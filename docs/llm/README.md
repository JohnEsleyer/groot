# LLM Contributor Guide — Groot (wgpu + GoScript)

Technical notes and conventions for LLMs working on the Groot engine.

---

## 1. Architecture Overview

Groot is a data-driven hybrid 2D/3D engine:
- **Data/Visuals** — Declarative RON files under `assets/prefabs/` and `assets/scenes/`.
- **Behavior** — GoScript files under `assets/scripts/`.
- **Presentation & ECS** — wgpu 0.19 (rendering) + winit 0.29 (windowing) + hecs 0.10 (ECS).

### Crate structure
- `groot-plugin-api` (library) — shared `GrootPlugin` trait and `PluginManager`
- `groot` (binary + library) — the game engine: wgpu rendering, hecs ECS, GoScript VM
- `groot-plugin-audio` (library) — sample audio plugin
- `groot-plugin-gizmos` (library) — sample gizmos plugin
- `groot-plugins` — plugin registry index

### Key files
| File | Purpose |
|------|---------|
| `assets/config.ron` | Project config: window, render settings, initial scene path |
| `assets/prefabs/*.prefab.ron` | RON prefabs: 2D/3D visuals, colliders, scripts, child hierarchies |
| `assets/scenes/*.scene.ron` | Scene layout: camera, lighting, entity instantiation |
| `src/main.rs` | Initializes winit window + wgpu context, runs main event loop |
| `src/lib.rs` | Library root: exposes `plugin`, `ecs`, `render`, `assets` modules |
| `src/plugin.rs` | Re-exports `GrootPlugin` and `PluginManager` from `groot-plugin-api` |
| `src/render/` | wgpu rendering: camera, mesh pipeline, 3D PBR |
| `src/ecs/` | hecs ECS components and queries |
| `src/script/` | GoScript VM host, input tracking, script execution |
| `src/groot_module.rs` | Stateless utility functions (math, logging) |
| `src/bin/cli.rs` | `groot` CLI — scaffolds, runs, builds, manages plugins |

---

## 2. Architecture Rules (Never Violate)

- **Scripts manipulate data, never rendering.** Debug visualization is driven by declared data.
- **`GrootScriptHost` uses `Rc<RefCell<...>>` internally.** It is NOT `Send`/`Sync`.
- **`groot_module.rs` is stateless.** Pure utility methods only.
- **No global entity-state map.** Self state flows through thread-local scratch (`CURRENT_ENTITY` + `CURRENT_STATE`), synced to/from ECS components each frame.
- **`static Mutex` only for cross-system queues:** `SPAWN_REQUESTS`, `SCRIPT_EVENTS`.
- **Visuals are RON data.** To change how something looks, edit a `.prefab.ron` file.
- **Plugins depend on `groot-plugin-api`, NOT on `groot` directly.** This avoids cyclic dependencies.

---

## 3. RON Asset Schemas

### Config (`assets/config.ron`)
```ron
(
    project: (name: "...", version: "..."),
    window: (title: "...", width: 1280.0, height: 720.0),
    render: (clear_color: Rgba(0.08, 0.09, 0.12, 1.0)),
    initial_scene: "assets/scenes/main.scene.ron",
)
```

### Prefab (`assets/prefabs/*.prefab.ron`)
```ron
(
    name: "entity_name",
    script: Some("assets/scripts/file.gos"),
    transform: (
        position: (x, y, z),
        rotation: (pitch, yaw, roll),  // Euler degrees
        scale: (sx, sy, sz),
    ),
    visual: Some(MeshPbr(
        shape: Cuboid(x: 1.0, y: 1.0, z: 1.0),
        material: (color: Rgba(r, g, b, a), roughness: 0.5, metallic: 0.0),
    )),
    collider: Some(Box3D(x: 1.0, y: 1.0, z: 1.0)),
    children: [ ... ],
)
```

### Scene (`assets/scenes/*.scene.ron`)
```ron
(
    name: "Scene Name",
    environment: (
        ambient_light: Some((color: Rgba(...), brightness: 0.25)),
        camera: Some(Perspective3D(fov: 60.0, position: (...), look_at: (...))),
    ),
    entities: [
        (prefab: "path.ron", entity_id: Some(1), tag: "Tag", transform_override: None),
    ],
)
```

### Visual Types
- `Sprite { size, color }` — 2D sprite
- `Text { value, size, color }` — 2D text overlay
- `MeshPbr { shape, material }` — 3D PBR mesh (Cuboid, Sphere, Cylinder, Plane)
- `Light(Point | Directional)` — 3D light sources

### Collider Types
- `Box2D { width, height }` — 2D AABB
- `Box3D { x, y, z }` — 3D AABB
- `Sphere3D { radius }` — 3D sphere

---

## 4. Host Function API Reference

### 3D Self-context (reads/writes thread-local scratch)
```
groot.GetSelfPosition() []float64      // [x, y, z]
groot.SetSelfPosition(x, y, z float64)
groot.GetSelfRotation() float64        // yaw
groot.SetSelfRotation(yaw float64)
groot.SetSelfRotation3D(pitch, yaw, roll float64)
groot.GetSelfScale() []float64         // [sx, sy, sz]
groot.SetSelfScale(sx, sy, sz float64)
groot.SetSelfColor(r, g, b, a float64)
groot.SetSelfMaterialColor(r, g, b, a float64)
groot.SetSelfCollider(x, y float64)    // Box2D
groot.SetSelfCollider(x, y, z float64) // Box3D
groot.DestroySelf()
```

### Input
```
groot.GetAxis(axis string) float64     // "Horizontal", "Vertical"
groot.IsKeyDown("Space") bool
groot.IsKeyPressed("Space") bool
groot.GetMouseWorld() []float64
groot.IsMouseDown(0|1|2) bool
groot.IsMousePressed(0|1|2) bool
```

### Entity queries (per-frame snapshot)
```
groot.GetEntityPosition(id int) []float64  // [x, y, z]
groot.GetDistance(idA, idB int) float64    // 3D distance
```

### Commands
```
groot.SpawnPrefab(path string, x, y, z float64, tag string)
groot.SpawnEntity(script string, x, y, z float64, tag string)
groot.EmitEvent(name string, payload float64)
```

### Stateless utilities (groot_module.rs)
```
groot.Log(msg), groot.Warn(msg), groot.Error(msg)
groot.Clamp(v, min, max), groot.Lerp(a, b, t)
groot.GetDistance2D(x1, y1, x2, y2)
groot.RectsOverlap(...), groot.CirclesOverlap(...), groot.CircleHitsRect(...)
```

---

## 5. Engine Pipeline (per frame)

1. **Window event** — winit dispatches input events
2. **Input sync** — `InputState` snapshots keyboard/mouse state
3. **Script hot reload** — `GrootScriptHost` recompiles changed `.gos` files
4. **Script execution** — runs `OnUpdate` per script entity, syncs Transform/Sprite/Material/Collider
5. **ECS world update** — hecs processes entity mutations
6. **Render** — wgpu submits draw calls (3D PBR mesh pipeline, 2D sprites, text)

---

## 6. Hot-Reload Workflow

**GoScript:** Edit `.gos` in `assets/scripts/`, save, VM recompiles, globals preserved.
**RON Prefabs:** Edit `.prefab.ron` in `assets/prefabs/`, save, engine re-spawns visuals (meshes, materials, colliders) while preserving `GoScriptComponent` state.

---

## 7. Adding a New Feature Checklist

1. **Read existing host functions** in `src/script/host.rs` — mimic the `groot.SnakeCaseName` pattern.
2. **Pure math/log?** Add to `src/groot_module.rs`.
3. **Per-entity data?** Use `CURRENT_ENTITY` / `CURRENT_STATE` thread-local scratch.
4. **Cross-system command?** Use `static Mutex` queues.
5. **New visual?** Define a `.prefab.ron` file; don't hardcode in Rust.
6. **New plugin?** Create a crate depending on `groot-plugin-api`, implement `GrootPlugin`.
7. **Run `cargo check`** — zero errors required.
8. **Test** with a `.gos` script and verify runtime behavior.

---

## 8. Graphics Are Data

Users define **no Rust** to make visuals: prefabs in RON files map names to
meshes/sprites/lights and optionally a behavior script. The engine's
scene spawner and ECS systems turn that data into wgpu draw calls.
Scripts are pure logic; the engine handles rendering.

---

## 9. 3D GoScript Bindings

The `CURRENT_STATE` thread-local scratch supports full 3D:
- Position: `(x, y, z)`
- Rotation: `(pitch, yaw, roll)` in Euler degrees
- Scale: `(scale_x, scale_y, scale_z)`
- Material color: `(r, g, b, a)` — applies to both Sprite and PBR material

Scripts can set 3D colliders (`Box3D`) via `groot.SetSelfCollider(x, y, z)`.
