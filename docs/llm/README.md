# LLM Contributor Guide — Groot (Bevy)

Technical notes, gotchas, and conventions for LLMs working on the Groot Bevy engine.

---

## 1. Architecture Overview

Groot is a **data-driven hybrid game engine**: scripts own *behavior and data*,
the host engine owns *representation and rendering*.

- **Data/Behavior** — GoScript (`.gs`) files manipulate entity data: position,
  velocity, color, scale, collider, state, spawns, and events.
- **Presentation** — Bevy ECS + sprites/UI. Visuals are declared as *data* in
  `groot.toml` prefabs and spawned by the engine. Scripts never issue draw
  calls — they write components/data that Bevy renders.

### Crate structure
- `goscript` (library) — the GoScript VM, compiler, hot-reload engine
- `groot` (binary) — the Bevy game engine that embeds goscript

### Key files
| File | Purpose |
|------|---------|
| `groot.toml` | Project config: window, `[[prefab]]` visuals, `[[scene.entity]]` initial entities |
| `src/main.rs` | Loads `groot.toml`, builds the window, inserts `GrootConfig`, adds the plugin |
| `src/groot_plugin.rs` | Core Bevy plugin: components, host functions, ECS systems, prefab/scene spawning |
| `src/groot_module.rs` | Stateless `GrootModuleExt` trait (math, collision, logging) — pure functions, no state, no rendering |
| `src/bin/cli.rs` | `groot` CLI — scaffolds/runs/builds projects |

---

## 2. Architecture Rules (Never Violate)

- **Scripts manipulate data, never rendering.** No `Draw*`, no immediate-mode
  geometry from scripts. Debug visualization is driven by declared data:
  scripts set a `Collider` component (via `groot.SetSelfCollider`) and the
  engine draws the overlay when `DebugRender.show_colliders` is on.
- **`GrootScriptHost` does NOT derive `Resource`.** goscript uses `Rc`/`RefCell`
  which are not `Send`/`Sync`. Use `NonSend`/`NonSendMut` in Bevy systems.
- **`GrootModuleExt` is stateless.** It only adds pure utility methods to
  `VirtualMachine` — no `self`, no entity lookup, no rendering.
- **No global entity-state map.** `ENTITY_STATES` was removed. Self state flows
  through a thread-local scratch (`CURRENT_ENTITY` + `CURRENT_STATE`) that
  `script_execution_system` copies from/to the entity's ECS components every
  frame. The ECS components are the source of truth.
- **`static Mutex` only for cross-system queues** that other systems drain:
  `SPAWN_REQUESTS`, `SCRIPT_EVENTS`. Entity state never uses them.
- **Foreign-host calls cross the VM boundary via `static`/thread-local**
  because native bindings can't borrow the Bevy world.
- **Visuals are data.** To change how something looks, edit a prefab in
  `groot.toml` — not Rust. Use `kind = "pipe"` / `kind = "score"` for special
  host components.

---

## 3. File-Specific Gotchas

### `groot.toml`
- `[[prefab]]` entries map a name to a `sprite`/`text`, optional `script`
  (behavior), optional `size` (collider box), and optional `z`.
- `[[scene.entity]]` lists initial entities by prefab name + position.
- Wire-field naming in serde matches the TOML header: the struct field for
  prefabs is `prefabs` deserialized with `#[serde(rename = "prefab")]`, and the
  scene list is `entities` with `#[serde(rename = "entity")]`.

### `groot_plugin.rs`
- `GrootScriptHost` wraps `HashMap<String, HotReloadEngine>` keyed by script path.
- `CURRENT_ENTITY` / `CURRENT_STATE` — thread-locals set during each script call.
- `SPAWN_REQUESTS: Mutex<Vec<SpawnRequest>>` — `Prefab`/`Script` variants.
- All host functions receive `&[Value]` and return `Value`. Use
  `args.first().and_then(|v| v.as_number())` for arguments.
- `groot.GetSelfPosition()` / `groot.GetEntityPosition(id)` return
  `Value::Slice`. Slice indexing in complex argument lists is **safe** now (the
  VM was hardened: operand-stack restoration on error, negative-index errors,
  and stack-underflow detection in `SetLocal`/`Call`/`CallMethod`).

### `groot_module.rs`
- `GrootModuleExt` is a trait on `VirtualMachine` — pure math utilities only.
- Names are engine-neutral (`RectsOverlap`, `CirclesOverlap`, `CircleHitsRect`).

### `main.rs`
- Loads `GrootConfig::load("groot.toml")`, inserts it as a resource, and lets
  `spawn_scene_system` (registered by the plugin on `Startup`) do the spawning.
- Bevy 0.13: `Camera2dBundle::default()`.

---

## 4. Host Function API Reference

All `groot.*` functions are registered in `GrootScriptHost::ensure_engine`
(`groot_plugin.rs`) plus the stateless `groot_module.rs` utilities.

### Logging
```
groot.Log(msg string)
groot.Warn(msg string)
groot.Error(msg string)
```

### Entity data (self — reads/writes the thread-local data scratch)
```
groot.GetSelfEntity() int              // current entity ID
groot.GetSelfPosition() []float64      // [x, y]
groot.SetSelfPosition(x, y float64)
groot.GetSelfRotation() float64
groot.SetSelfRotation(r float64)
groot.GetSelfScale() []float64         // [x, y]
groot.SetSelfScale(sx, sy float64)
groot.SetSelfColor(r, g, b, a float64)
groot.SetSelfCollider(w, h float64)    // hitbox data — engine owns its usage/rendering
groot.DestroySelf()
```

### Input
```
groot.GetAxis(axis string) float64     // "Horizontal", "Vertical"
groot.IsKeyDown("Space") bool
groot.IsKeyPressed("Space") bool       // just pressed this frame
groot.GetMouseWorld() []float64        // cursor in world coords
groot.IsMouseDown(0|1|2) bool          // 0 left, 1 right, 2 middle
groot.IsMousePressed(0|1|2) bool
```

### Entity queries (resolved from a per-frame snapshot)
```
groot.GetEntityPosition(id int) []float64
groot.GetDistance(idA, idB int) float64
```

### Commands
```
groot.SpawnPrefab(name string, x, y float64, tag string)   // spawn a prefab from groot.toml
groot.SpawnEntity(script string, x, y float64, tag string) // spawn a raw scripted entity
groot.PlaySound(name string)                                // logs for now
groot.EmitEvent(name string, payload float64)
```

### Math / collision (stateless utilities in groot_module.rs)
```
groot.Clamp(v, min, max float64) float64
groot.Lerp(a, b, t float64) float64
groot.GetDistance2D(x1, y1, x2, y2 float64) float64
groot.RectsOverlap(x1, y1, w1, h1, x2, y2, w2, h2 float64) bool
groot.CirclesOverlap(x1, y1, r1, x2, y2, r2 float64) bool
groot.CircleHitsRect(cx, cy, r, rx, ry, rw, rh float64) bool
```

### Game-write demos (routed to tagged entities by the execution system)
```
groot.SetPipePosition(idx int, x, gapY, gapSize float64) // moves PipeIndex entities
groot.SetScoreDisplay(score, best int)                    // updates the ScoreText entity
```

---

## 5. Entity Setup (GoScript Side)

Entities get their visual from a `[[prefab]]` and their behavior from an
optional `script`. The script never mentions meshes/colors-as-visuals; it
mutates *data*.

### Minimal script template
```go
type Player struct { Speed float64 }
var self = Player{Speed: 300.0}

func OnUpdate(dt float64) {
    var pos = groot.GetSelfPosition()
    var move = groot.GetAxis("Horizontal")
    groot.SetSelfPosition(pos[0] + move*self.Speed*dt, pos[1])

    // Declare hitbox data; the engine draws/handles it. We never draw.
    groot.SetSelfCollider(32.0, 32.0)
}
```

### Script conventions
- `OnUpdate(dt float64)` — called every frame with delta time.
- Receiver methods: `func (p *Player) TakeDamage(n int)` — call internally.
- Global variables are preserved across hot-reloads.
- `groot.Log(...)` / `groot.Warn(...)` write to the Bevy console.

---

## 6. Bevy System Pipeline

Systems execute in this order each frame (all chained in `Update`):

1. `script_hot_reload_system` — reloads scripts when files change
2. `script_input_sync_system` — snapshots keyboard/mouse state once per frame
3. `script_execution_system` — for each script entity: copy ECS → scratch run
   `OnUpdate`, copy scratch → ECS (Transform/Sprite/ScriptTransform/ScriptColor/
   Collider), despawn on request; then applies queued `GameWrite`s to tagged
   entities (pipes/score)
4. `handle_spawn_requests_system` — spawns `SPAWN_REQUESTS` (prefab/script)
5. `handle_script_events_system` — logs `SCRIPT_EVENTS`
6. `render_collider_debug_system` — draws a wireframe box per `Collider` when
   `DebugRender.show_colliders`

`Startup` runs `spawn_scene_system` to build the initial scene from `groot.toml`.

---

## 7. Common Pitfalls

### Slice indexing in argument lists (fixed)
Indexing slices inside multi-arg host calls — e.g.
`groot.RectsOverlap(px, py, pos[0], pos[1], ...)` — is now safe. The VM was
hardened to restore the operand stack to pre-call depth on script errors, reject
negative indices, and detect stack misalignment. Prefer extracting locals for
readability:
```go
var px = pos[0]
var py = pos[1]
```

### GoScript stdlib imports
- `math.Sin`, `fmt.Sprintf`, `rand.Float()`, `rand.Intn(n)`, `time.Delta` are
  built-in — do NOT use `import` statements.

### Bevy 0.13 specifics
- `gizmos.rect_2d(position, rotation, size, color)` for the collider overlay.
- Camera bundle: `Camera2dBundle::default()`.

### `Value::Slice` access
```go
var pos = groot.GetSelfPosition()
var px = pos[0]  // safe
var sum = pos[0] + pos[1]  // safe (regression-tested)
```

---

## 8. Hot-Reload Workflow

1. Edit any `.gs` file in `assets/scripts/`.
2. Save. `script_hot_reload_system` detects the change.
3. The engine recompiles with `HotReloadEngine::reload_if_changed()`.
4. Live global values (e.g. `var self = Player{...}`) are preserved.
5. Next tick executes the updated `OnUpdate` immediately.

**Note:** Hot-reload is per-script-path. Each script path gets its own
`HotReloadEngine` instance.

---

## 9. Adding a New Feature Checklist

1. **Read existing host functions** in `groot_plugin.rs` — mimic the
   `groot.SnakeCaseName` pattern.
2. **Pure math/log?** Add it to `groot_module.rs` (stateless, no rendering).
3. **Per-entity data?** Use the `CURRENT_ENTITY` / `CURRENT_STATE` thread-local
   scratch (as `groot.SetSelf*` does). Do NOT add a global state map.
4. **Cross-system command?** Use the existing `static Mutex` queues
   (`SPAWN_REQUESTS`, `SCRIPT_EVENTS`).
5. **New visual?** Define a prefab in `groot.toml`; don't hardcode in Rust.
6. **Run `cargo build`** — zero warnings required before commit.
7. **Test** with a `.gs` script call and verify runtime behavior.

---

## 10. Graphics Are Data

Users define **no Rust** to make visuals: prefabs in `groot.toml` map names to
sprite size/color (and optionally a behavior script). The engine's
`spawn_scene_system` / `handle_spawn_requests_system` turn that data into Bevy
bundles. This keeps scripts as pure logic and preserves Bevy's batching and
render graph.