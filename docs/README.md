# Groot Engine Documentation

Welcome to the **Groot Engine (Ube Edition)** documentation. Groot is a zero-GC, data-oriented 2D/3D game engine compiling directly to WebAssembly components (`wasi2`) that run atop a high-performance native host (`wgpu` + `winit` + `wasmtime`).

---

## Documentation Index

### 1. Architecture & Core Concepts
- [System Architecture](architecture/01-system-overview.md) — The guest-host boundary, WASI 0.2 Canonical ABI, and separation of concerns.
- [Memory Management & Zero-GC](architecture/02-zero-gc-memory.md) — Regional bump arenas (`arena { ... }`), generational entity pools (`Pool[T]`), and affine ownership.
- [Host-Guest HAL ABI](architecture/03-host-abi.md) — The `ube:graphics` contract, windowing, and GPU batch submission.

### 2. Developer Guides
- [Getting Started & Project Scaffolding](guides/01-getting-started.md) — Compiling Ube games to `.wasm` and running them with `groot-host`.
- [2D Batch Rendering Pipeline](guides/02-2d-batch-rendering.md) — Quads, vertex structures (`x, y, u, v, r, g, b, a`), and texture atlases.
- [Generational ECS & Entity Management](guides/03-generational-ecs.md) — Structuring game state with zero-GC entity pools and handles.
- [Physics & Collision Detection](guides/04-physics2d.md) — AABB overlaps, MTV manifolds, and raycasting.
- [Camera & Viewport Coordinates](guides/05-camera-and-viewports.md) — World-to-screen transforms, pan, zoom, and projections.

---

## Quick Reference

| Package | Purpose | Imports |
| :--- | :--- | :--- |
| `pkgs/groot/app` | Game lifecycle, frame timing, and main event loops | `render`, `input`, `platform`, `time`, `color` |
| `pkgs/groot/render` | Batch quad generation, 8-float vertex buffers, Camera2D | `math`, `color` |
| `pkgs/groot/ecs` | Archetype-ready generational entity pools and handles | `math`, `color` |
| `pkgs/groot/physics2d` | AABB intersection manifolds, raycasting, and MTV normal resolution | `math` |
| `pkgs/groot/math` | `Vec2`, `Vec3`, `Mat4`, `Rect`, `Transform` | *pure computation* |
| `pkgs/groot/color` | `Color` RGBA math and presets | *pure computation* |
| `pkgs/groot/input` | Keyboard and mouse event tracking | *pure computation* |
| `pkgs/groot/assets` | Texture atlas slicing and sprite regions | *pure computation* |
| `pkgs/groot/platform` | Direct WASI 0.2 `ube:graphics` foreign imports | `ube:graphics/gpu`, `ube:graphics/window` |
