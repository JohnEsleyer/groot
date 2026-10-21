# 04 — The Script Runtime in the Engine

This document describes how Groot runs scripts: one VM instance per entity,
how script state reaches the ECS, and what happens in a frame.

## The problem: shared state

A script's top-level variables live in the VM's global table:

```go
// assets/scripts/pipe.gos
type PipeController struct {
    Speed float64
    Hit   bool
}

var self = PipeController{
    Speed: 3.2,
    Hit:   false,
}

func OnUpdate(dt float64) {
    …
    if hit && !self.Hit {
        self.Hit = true
        groot.SetGameOver(true)
    }
}
```

A flappy-bird scene spawns *many* pipes from that one prefab. If all pipes
shared a VM, they would share one `self` struct: the first pipe to be hit
would set `self.Hit = true` for **every** pipe, and each subsequent pipe would
skip its own collision test.

The fix is to give every entity its own VM instance.

## The design

```
GrootScriptHost
 └── scripts: HashMap<String, LoadedScript>      one entry per script file
      ├── engine: HotReloadEngine                 the compiled script (template)
      ├── metadata: ScriptMetadata                which hooks exist
      └── instances: HashMap<u32, VirtualMachine> one VM per entity
```

* The **template** VM owns the compiled script and is the single place where
  the file is compiled and hot-reloaded.
* Each **instance** VM is a full VM with the same host API and the same
  functions, but with its own global table.

### Creating an instance

```rust
pub fn ensure_instance(&mut self, script_path: &str, entity_id: u32) -> &mut VirtualMachine {
    self.ensure_engine(script_path);           // compile the template if needed

    let loaded = self.scripts.get_mut(script_path).expect("…");
    if !loaded.instances.contains_key(&entity_id) {
        let mut vm = VirtualMachine::new();
        register_host_api(&mut vm, &self.input, &self.plugin_mgr);
        for (name, value) in loaded.engine.vm.globals.iter() {
            vm.globals.insert(name.clone(), value.deep_clone());  // ← the isolation
        }
        loaded.instances.insert(entity_id, vm);
    }
    loaded.instances.get_mut(&entity_id).expect("…")
}
```

The `deep_clone()` is what makes the state private: structs, slices and maps
are copied recursively (and cyclic references are preserved, so a
scene-graph-like state clones safely).

### Host API per instance

Every VM — template or instance — gets the complete `groot.*` surface:

```rust
fn register_host_api(vm: &mut VirtualMachine, input: &Rc<InputState>, plugin_mgr: &PluginManager) {
    vm.register_groot_module();                     // stateless helpers
    plugin_mgr.register_all_script_bindings(vm);    // plugin packages
    // … entity context, input, camera, spawning …
}
```

The functions read the "current entity" from a thread-local, so the same
closure works for every instance.

## The frame

`update_scripts()` runs once per frame, before rendering:

```rust
pub fn update_scripts(host: &mut GrootScriptHost, world: &mut World, dt: f64) {
    host.reload_all();                              // 1. hot-reload changed scripts
    host.plugin_mgr.update_all(world, dt);          // 2. plugin per-frame work

    // 3. snapshot positions + tags for the queries scripts make
    ENTITY_POSITIONS.with(/* … */);
    TAG_POSITIONS.with(/* … */);

    // 4. run every script entity
    for (entity, (comp, tf, vis3d, vis2d, vistext, collider)) in
        world.query_mut::<(&GoScriptComponent, &mut Transform3D, …)>()
    {
        // 4a. load the entity's world state into the thread-local
        CURRENT_ENTITY.with(|c| c.set(comp.entity_id));
        CURRENT_STATE.with(|st| { st.borrow_mut().x = tf.position.x; /* … */ });

        // 4b. run the script on this entity's own VM
        let vm = host.ensure_instance(&comp.script_path, comp.entity_id);
        if vm.globals.contains_key("OnUpdate") {
            vm.set_delta_time(dt);
            vm.call("OnUpdate", vec![Value::Float(dt)]);
        }

        // 4c. write the (possibly modified) state back into the ECS
        CURRENT_STATE.with(|st| {
            tf.position = glam::Vec3::new(s.x, s.y, s.z);
            tf.rotation = glam::Vec3::new(s.pitch, s.yaw, s.roll);
            v3.color = [s.color.0, s.color.1, s.color.2, s.color.3];
            *col = s.collider;
        });
    }

    // 5. apply deferred commands (spawn, despawn, text, …)
    // 6. release the instances of entities that no longer exist
}
```

### The thread-local bridge

Scripts cannot hold a pointer to an ECS component, and host closures cannot
borrow the world. Groot bridges the two with two thread-locals:

```rust
thread_local! {
    static CURRENT_ENTITY: Cell<u32>;
    static CURRENT_STATE: RefCell<EntityState>;
}
```

* **In** — before the call, the entity's transform/colour/collider are copied
  into `CURRENT_STATE`.
* **Out** — after the call, `CURRENT_STATE` is copied back into the ECS
  components.

That is why `groot.SetSelfPosition(…)` works without any lookup: it writes to
the thread-local, and the frame loop copies the value back.

`CURRENT_ENTITY` additionally identifies the entity for the queries:
`groot.GetEntityPosition(id)`, `groot.GetTagPositions(tag)`.

### Deferred commands

Some operations cannot run while the ECS is being iterated (spawning or
despawning entities would invalidate the query). Those go through process-wide
queues:

```rust
static SPAWN_REQUESTS: Mutex<Vec<SpawnRequest>>;
static DESPAWN_TAGS: Mutex<Vec<String>>;
static SET_TEXT_BY_TAG_REQUESTS: Mutex<Vec<(String, String)>>;
```

They are drained and applied after all scripts have run for the frame.

## Lifecycle

| Moment | What happens |
| --- | --- |
| Prefab spawn | The entity gets a `GoScriptComponent { script_path, entity_id, tag }`. |
| First frame | `ensure_instance` creates the VM instance and seeds it with a deep copy of the script's globals. |
| Every frame | `OnUpdate(dt)` runs on the instance; state flows in and out through the thread-local. |
| Script edit | The template recompiles; the new function bodies are hot-swapped into every instance; their data is preserved. |
| `groot.DestroySelf()` | The entity is marked and despawned at the end of the frame. |
| Despawn | The instance is pruned, releasing the entity's script state. |

## Cost model

One VM per entity is not free: each instance carries its own global table,
stack and host-function table. In practice:

* The per-instance overhead is a `HashMap` of globals plus the standard
  library registrations — a few kilobytes per entity.
* Script *execution* cost is unchanged: the same bytecode runs either way.
* The benefit is correctness: entity state can no longer leak between
  entities, which is the whole point of a behaviour system.

If a project has thousands of script entities and the memory matters, the
alternative is an explicit state table (`map[entityID]State`) inside a single
VM — more bookkeeping for the script author, but one VM.

## Testing the runtime

The isolation guarantees are covered by tests in `src/script/host.rs`:

* two entities running the same script keep separate top-level state,
* a hot reload swaps the code but keeps each instance's state,
* despawning an entity releases its instance.
