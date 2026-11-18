# 05 — Engine Overview

The engine around the VM: ECS, rendering, assets and the frame loop.

```
                    ┌──────────────────────────────────────────┐
   winit events ───►│ Event loop (src/lib.rs, src/platform/)   │
                    └───────────────┬──────────────────────────┘
                                    │  WindowEvent::RedrawRequested
                                    ▼
        ┌───────────────────────────────────────────────────────┐
        │ 1. script::sync_camera_commands()                     │
        │ 2. script::update_scripts()      ← GoScript (see 04)  │
        │ 3. camera uniform upload                             │
        │ 4. build the frame's draw lists                       │
        │ 5. encode + submit                                   │
        └───────────────────────────────────────────────────────┘
```

## Crates

| Crate | Role |
| --- | --- |
| `groot` (lib + bin) | The engine: wgpu rendering, hecs ECS, GoScript host. |
| `goscript` | The scripting language (path-patched from the sibling checkout). |

## The ECS

`hecs` 0.10 with a small set of components (`src/ecs/components.rs`):

| Component | Purpose |
| --- | --- |
| `Transform3D` | Position, Euler rotation, scale; produces the model matrix. |
| `Visual3D` | A 3D mesh (cuboid or sphere) with a colour. |
| `Visual2D` | A sprite: size, colour, optional texture, layer. |
| `VisualText` | A text quad: value, size, colour, layer. |
| `Collider` | `None`, `Box2D`, `Box3D`, `Sphere3D`. |
| `GoScriptComponent` | The script path, the engine entity id and the tag. |
| `TagComponent` | A tag, for tag-based queries. |

Entities are queried by component tuple, e.g. `world.query_mut::<(&GoScriptComponent, &mut Transform3D, …)>()`.

## Rendering

`src/render/` is a small wgpu 0.19 renderer.

* **`context.rs`** — the `RenderContext`: device, queue, surface, depth
  texture, and the resize logic. On Android it is created in the
  `Event::Resumed` handler — the native window surface does not exist before
  that (see `case-studies/001`).
* **`pipeline_2d.rs` / `pipeline_3d.rs`** — the two render pipelines, each with
  a camera bind group (group 0) and a per-object model bind group (group 1).
* **`uniform_pool.rs`** — the pooled uniform buffers (see below).
* **`mesh.rs`** — the cube and quad meshes.
* **`texture.rs`** — texture loading and caching, plus the generated
  text/glyph textures.
* **`camera.rs`** — `Camera2D` / `Camera3D` and their uniform blocks.
* **`font.rs`** — the bitmap font used to rasterise text.

### Pooled uniform buffers

Every visible object needs a uniform buffer holding its model matrix and
colour. Creating a fresh GPU buffer *and* bind group per object per frame
churns the GPU allocator, so the buffers are pooled:

```rust
st.pipeline_3d.uniform_pool.reset();      // start of frame
let bind_group = st.pipeline_3d.create_model_bind_group(
    &device, &queue, tf.matrix(), vis.color,
);
```

The pool owns one slot per draw call. A slot's buffer is created the first
time it is needed; afterwards only the contents are rewritten with
`queue.write_buffer`. The bind groups are handed out behind an `Rc`
(`wgpu::BindGroup` is not `Clone` in wgpu 0.19) so the frame can collect them
without borrowing the pool.

The result: a steady-state frame allocates **no** GPU buffers, no matter how
many entities are on screen.

### Frame structure

One render pass, two pipelines:

1. **3D pass** — depth tested, `Visual3D` entities, the cube mesh.
2. **2D pass** — alpha blended, `Visual2D` sprites and `VisualText` quads,
   sorted by `layer` so backgrounds sit behind the player.

## Assets

`src/assets/` owns everything that is loaded from disk:

* **`ron_loader.rs`** — parses `.prefab.ron` and `.scene.ron`. A prefab
  declares a transform, a visual, a collider, an optional script path and
  optional children; a scene declares the camera, the environment and the
  prefab instances to spawn.
* **`crypto.rs`** — the asset encryption used in release builds (a
  ChaCha20-inspired XOR stream cipher with a CRC32 integrity header, keyed by
  a compile-time secret or `GROOT_ENCRYPTION_KEY`).
* **`embed.rs`** — embeds assets into the binary for release builds.
* **`pak.rs`** — packs assets into an archive.
* **`spawner.rs`** — instantiates prefabs into the ECS.

In debug builds assets are read from `assets/` directly (the loader returns the
original path so the engine can pick up edits); in release builds they are
encrypted and embedded into the binary, and scripts are compiled from the
in-memory source with no temporary file ever written to disk.

## The CLI

`src/bin/cli.rs` is the `groot` binary:

| Command | Purpose |
| --- | --- |
| `groot new <name>` | Scaffold a project (assets, config, scenes). |
| `groot run` | Run the demo (`groot-demo` binary). |
| `groot build [--android\|--desktop]` | Build a release binary. |
| `groot doctor` | Check the toolchain and the project layout. |
| `groot generate script\|prefab\|scene` | Scaffold a script/prefab/scene. |
| `groot pack` | Pack the assets into an archive. |
| `groot info` | Show project information. |
| `groot completions <shell>` | Shell completions (bash/zsh/fish). |

## Platforms

`src/platform/` isolates the platform-specific entry points:

* **`desktop.rs`** — the desktop event loop.
* **`android.rs`** — the Android entry point (`android_main`), the activity
  glue and the logger. The render context is created in `Event::Resumed`.

Both platforms share the same rendering, ECS and scripting code.

## Data-driven by design

The split the whole engine is built around:

| Concern | Owner | Format |
| --- | --- | --- |
| How something looks | The engine | `.prefab.ron` / `.scene.ron` |
| How something behaves | The script | `.gos` |
| Which entities exist | The scene | `.scene.ron` |

A designer changes the look by editing RON and the behaviour by editing Go —
neither requires touching Rust, and both hot-reload.
