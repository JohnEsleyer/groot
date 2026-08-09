# LLM Contributor Guide — Groot (Bevy)

Technical notes, gotchas, and conventions for LLMs working on the Groot Bevy engine.

---

## 1. Architecture Overview

Groot is a **hybrid component-behavior game engine** combining:
- **Bevy ECS** — entity isolation, hot-reloadable GoScript components
- **GoScript VM** — game logic in `.gs` files (Go-like scripting language)

It merges Raylib-style utility ergonomics with ECS entity isolation.

### Crate structure
- `goscript` (library) — the GoScript VM, compiler, hot-reload engine
- `groot` (binary) — the Bevy game engine that embeds goscript

### Key files
| File | Purpose |
|------|---------|
| `src/main.rs` | Bevy app setup, entity spawning |
| `src/groot_plugin.rs` | Core Bevy plugin: host functions, ECS systems, context dispatch |
| `src/groot_module.rs` | Stateless `GrootModuleExt` trait (math, collision, logging) |

---

## 2. Architecture Rules (Never Violate)

- **`GrootScriptHost` does NOT derive `Resource`.** goscript uses `Rc`/`RefCell` which are not `Send`/`Sync`. Use `NonSend` and `NonSendMut` for all Bevy systems.
- **`GrootModuleExt` is stateless.** It only adds utility methods to `VirtualMachine` — no `self` parameter needed at the call site.
- **`GrootPlugin` must be inserted BEFORE `DefaultPlugins`.** The plugin registers `declare_package("groot")` which must happen before any `.gs` files are parsed.
- **`static` globals for state passing.** GoScript native bindings can't borrow the Bevy world. Use `static Mutex<...>` or `static RefCell<...>` for VM↔Bevy state crossing.
- **`CURRENT_ENTITY` thread-local.** Every script execution sets `CURRENT_ENTITY` before calling `vm.execute()`. Host functions read this to know which entity they're operating on.

---

## 3. File-Specific Gotchas

### `groot_plugin.rs`
- `GrootScriptHost` wraps `HashMap<String, HotReloadEngine>` keyed by script path.
- `ENTITY_STATES: LazyLock<Mutex<HashMap<i32, EntityState>>>` — shared world state (positions, colors, tags, visibility, event queues).
- `CURRENT_ENTITY: thread_local!` — set before each script execution.
- `SPAWN_REQUESTS: Mutex<Vec<SpawnRequest>>` — deferred entity spawns.
- All host functions receive `Vec<Value>` and return `Value`. Use `args[0].as_number()` for the first argument.
- `groot.GetPosition(entity_id)` returns `Value::Slice` (Rc<RefCell<Vec<Value>>>). You must `borrow()` the inner `Vec` to access elements.

### `groot_module.rs`
- `GrootModuleExt` is a trait on `VirtualMachine`, not a standalone struct.
- All methods are stateless utilities (no `self` context needed).
- Uses `rand` crate directly — no GoScript VM state involved.

### `main.rs`
- Bevy 0.13: `Camera2dBundle::default()` spawns camera.
- Entity spawning: `commands.spawn((GoScriptComponent::new(...), SpriteBundle { ... }))`.
- `GoScriptComponent::new(entity_id, script_path, tag)` — the entity ID is assigned by you, not auto-generated.

---

## 4. Host Function API Reference

All `groot.*` functions are registered as host functions in `groot_plugin.rs`.

### Logging
```
groot.Log(msg string)
groot.Warn(msg string)
groot.Error(msg string)
```

### Entity Context (self-operations)
```
groot.GetSelfEntity() int          // current entity ID
groot.GetSelfPosition() []float64  // [x, y]
groot.SetSelfPosition(x, y float64)
groot.SetSelfColor(r, g, b float64)
groot.SetSelfScale(sx, sy float64)
groot.SetSelfTag(tag string)
```

### Input
```
groot.GetKeyPressed() bool         // spacebar pressed this frame
groot.GetKeyReleased() bool
groot.GetKeyHeld() bool
groot.GetMouseButtonPressed() bool
groot.GetMouseButtonDown() bool
groot.GetMouseButtonReleased() bool
groot.GetMouseX(), GetMouseY() float64
groot.GetMouseWorldX(), GetMouseWorldY() float64
groot.GetAxis(axis string) float64 // "Horizontal", "Vertical", "Jump", "Fire"
groot.IsGamepadAvailable(pad int) bool
groot.GetGamepadAxisMovement(pad, axis int) float64
```

### Debug Drawing
```
groot.DrawDebugCircle(x, y, radius, r, g, b float64)
groot.DrawDebugRect(x, y, w, h, r, g, b float64)
groot.DrawDebugLine(x1, y1, x2, y2, r, g, b float64)
groot.DrawDebugText(x, y, size float64, r, g, b float64, msg string)
```

### Queries
```
groot.GetDistance(a, b int) float64    // distance between two entities
groot.GetEntity(id int) Entity         // returns Entity with GetPosition()
```

### Commands
```
groot.SpawnEntity(scriptPath string, x, y float64, tag string)
groot.DestroySelf()
```

### Event Bus
```
groot.EmitEvent(name string, data float64)
```
Events are stored in the entity's event queue and processed by the Bevy system in the next tick.

---

## 5. Entity Setup (GoScript Side)

Scripts define component structs and attach to entities via `GoScriptComponent::new()`.

### Minimal script template
```go
// Component struct (zero or more fields)
type MyComponent struct {
    Field1 float64
    Field2 int
}

// Global instance (singleton per entity)
var self = MyComponent{Field1: 1.0, Field2: 100}

// Called every tick by the engine
func OnUpdate(dt float64) {
    var pos = groot.GetSelfPosition()
    var newPos = pos[0] + self.Field1 * dt * groot.GetAxis("Horizontal")
    groot.SetSelfPosition(newPos, pos[1])
}

// Optional: receiver method called from Rust or other scripts
func (c *MyComponent) DoSomething(amount float64) {
    c.Field2 -= int(amount)
    if c.Field2 <= 0 { groot.DestroySelf() }
}
```

### Script conventions
- `OnUpdate(dt float64)` — called every frame. The engine passes delta time.
- `OnEvent(name string, data float64)` — called when the entity receives an event.
- Receiver methods: `func (c *Component) MethodName(args...)` — called via `vm.call("EntityID.MethodName", args)`.
- Global variables are preserved across hot-reloads.
- `groot.Log(...)` and `groot.Warn(...)` output to stderr/console.

---

## 6. Bevy System Pipeline

Systems execute in this order each frame:

1. `system_hot_reload` — reloads scripts when files change
2. `system_sync_input` — syncs keyboard/mouse state for GoScript bindings
3. `system_run_scripts` — executes each entity's `OnUpdate` with current delta time
4. `system_apply_transforms` — syncs `EntityState` positions/colors to Bevy `Transform`
5. `system_sync_entity_states` — syncs `GoScriptComponent` state to `ENTITY_STATES`
6. `system_render_gizmos` — draws debug shapes and text
7. `system_process_spawn_requests` — spawns entities from `SPAWN_REQUESTS`
8. `system_process_events` — calls `OnEvent` on entities that received events
9. `system_apply_destroy` — removes destroyed entities from `ENTITY_STATES`
10. `system_cleanup` — removes debug draws and event queues

---

## 7. Common Pitfalls

### Slice indexing in argument lists
```go
// ❌ BUG: arithmetic error when slice indexing inside many-arg function calls
groot.DrawDebugCircle(pos[0] + 10, pos[1], 5.0, 1, 1, 1)

// ✅ WORKAROUND: extract to local variables first
var x = pos[0]
var y = pos[1]
groot.DrawDebugCircle(x + 10.0, y, 5.0, 1.0, 1.0, 1.0)
```
This is a known issue with `GetIndex` opcode causing stack misalignment in complex argument lists.

### GoScript stdlib imports
- `math.Sqrt`, `math.Sin`, etc. are **built-in** — do NOT use `import` statements.
- `fmt.Sprintf` is built-in — no import needed.
- `rand.Float()`, `rand.Intn(n)` are built-in — no import needed.
- `time.Delta` is built-in — no import needed.

### Bevy 0.13 specifics
- `rect_2d()` takes 4 args: `(position, rotation, size, color)`.
- `HashMap::new()` can't be used in `static` — use `LazyLock<Mutex<HashMap::new()>>` instead.
- Camera bundle: `Camera2dBundle::default()`.

### `Value::Slice` access
```go
// Returns Value::Slice(Rc<RefCell<Vec<Value>>>)
var pos = groot.GetSelfPosition()

// ✅ Extract to local before indexing
var px = pos[0]  // works
var py = pos[1]  // works

// ❌ Nested indexing can fail
var px = pos[0] + pos[1]  // unreliable with many args
```

---

## 8. Hot-Reload Workflow

1. Edit any `.gs` file in `assets/scripts/`
2. Save the file
3. `system_hot_reload` detects the file change
4. The engine recompiles the script with `HotReloadEngine::reload_if_changed()`
5. Live global values (e.g. `var self = Player{...}`) are preserved across the swap
6. Next tick executes the updated code immediately

**Note:** Hot-reload is per-script-path. Each entity type has its own `HotReloadEngine` instance.

---

## 9. Adding a New Feature Checklist

1. **Read existing host functions** in `groot_plugin.rs` — mimic naming pattern: `groot.SnakeCaseName`.
2. **Read existing utilities** in `groot_module.rs` — stateless helpers on `VirtualMachine` trait.
3. **Check if you need entity context.** If the operation is entity-specific, use `CURRENT_ENTITY`.
4. **Check if you need Bevy world access.** If yes, use `static Mutex<...>` for state passing.
5. **Add the new host function** to `register_entity_api()` or `register_groot_api()`.
6. **Update `groot_module.rs`** only if adding stateless utility methods.
7. **Run `cargo build`** — zero warnings required before commit.
8. **Test with a `.gs` script** — add a call in a script and verify behavior.

---

## 10. Standalone vs Bevy Architecture

The `goscript` crate includes `examples/groot/` which is a **standalone** (non-Bevy) demo.

| Aspect | Standalone (examples/groot) | Bevy (012-groot) |
|--------|---------------------------|-------------------|
| State | `Rc<RefCell<>>` global `ScriptBridgeState` | `static Mutex<EntityState>` per entity |
| Threading | Single-threaded | Bevy ECS (NonSend) |
| Rendering | Fake (text output) | Bevy `SpriteBundle` + `Gizmos` |
| Spawning | `EngineCommand::SpawnEntity` deferred | `commands.spawn(...)` with bundles |
| Hot-reload | Same `HotReloadEngine` | Same `HotReloadEngine` |
| Scripts | Identical `.gs` files | Identical `.gs` files |

The standalone demo is useful for testing script logic without Bevy overhead.
