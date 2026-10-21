# Groot Documentation

Groot is a data-driven 2D/3D game engine: **wgpu** renders, **hecs** holds the
world state, and **GoScript** — an embeddable Go-syntax scripting VM written in
pure Rust — runs the gameplay logic.

These documents describe the two halves of that system:

| Section | What it covers |
| --- | --- |
| [`language/`](language/) | **The GoScript language** — syntax, semantics, values, standard library, and the `groot.*` host API that scripts use to talk to the engine. |
| [`architecture/`](architecture/) | **How it is built** — the compiler pipeline, the bytecode VM, hot reloading, the per-entity script runtime, and the engine itself (ECS, rendering, assets, plugins). |

Other material in this folder:

- [`llm/README.md`](llm/README.md) — contributor conventions and gotchas for
  people (or agents) editing the Groot codebase.
- [`case-studies/`](case-studies/) — post-mortems of real bugs (Android
  surface creation, asset embedding).

## Reading order

**New to the language?**

1. [`language/01-overview.md`](language/01-overview.md) — what GoScript is and
   how it differs from Go.
2. [`language/02-syntax-reference.md`](language/02-syntax-reference.md) — the
   complete syntax.
3. [`language/03-values-and-collections.md`](language/03-values-and-collections.md)
   — the value model, structs, slices, maps, strings.
4. [`language/04-standard-library.md`](language/04-standard-library.md) — the
   built-in standard library.
5. [`language/05-groot-host-api.md`](language/05-groot-host-api.md) — the
   `groot.*` API, entity lifecycle, and how scripts drive the engine.

**Want to understand the internals?**

1. [`architecture/01-compiler-pipeline.md`](architecture/01-compiler-pipeline.md)
   — source to bytecode.
2. [`architecture/02-bytecode-vm.md`](architecture/02-bytecode-vm.md) — the
   instruction set, operand encoding, and the runtime.
3. [`architecture/03-hot-reload.md`](architecture/03-hot-reload.md) — how a
   live edit replaces code without losing state.
4. [`architecture/04-script-runtime.md`](architecture/04-script-runtime.md) —
   per-entity VM instances and the frame loop.
5. [`architecture/05-engine-overview.md`](architecture/05-engine-overview.md) —
   the engine around the VM.

## The 30-second version

```
assets/scenes/*.scene.ron  ──►  hecs ECS (Transform3D, Visual2D/3D, Collider…)
assets/prefabs/*.prefab.ron ──►  prefab instantiation (visual + script path)
assets/scripts/*.gos        ──►  GoScript VM (one instance per entity)
                                         ▲
                                         └── groot.* host bindings ──┘
```

* **Scripts own behaviour and state.** A `.gos` file is attached to a prefab;
  every entity spawned from that prefab runs the file on its **own VM
  instance**, so its top-level state (`var self = ...`) is private to that
  entity.
* **The engine owns representation.** Visuals live in RON prefabs; the script
  only mutates transform/colour/collider data through the `groot.*` API.
* **Nothing runs on its own.** The VM executes exactly when the frame loop
  calls it — no threads, no GC pauses, no background work.
