# 01 — System Architecture & Overview

Groot inverts traditional game engine architectures by keeping the engine core in high-level compiled code while keeping the native host as a "dumb", high-throughput Hardware Abstraction Layer (HAL).

```
┌───────────────────────────────────────────────────────────────┐
│                     GAME APPLICATION (Ube)                    │
│        gameplay, scenes, animation, UI, spawn logic           │
└──────────────────────────────┬────────────────────────────────┘
                               │
┌──────────────────────────────▼────────────────────────────────┐
│                     GROOT CORE (Ube)                          │
│  math · color · render · input · ecs · physics2d · assets     │
│  time · app · platform                                        │
└──────────────────────────────┬────────────────────────────────┘
                               │ WASI 0.2 component imports
┌──────────────────────────────▼────────────────────────────────┐
│                  NATIVE HOST (Rust)                           │
│  winit window/events · wgpu sprite batches · wasmtime + WASI  │
└───────────────────────────────────────────────────────────────┘
```

## Why Ube + WebAssembly?

1. **Zero Garbage Collection Stutter**:
   - In languages like C# or Go, the runtime garbage collector halts frame execution to mark and sweep objects. Ube features regional bump arenas (`arena { ... }`) and affine type semantics, delivering predictable 60/120 FPS frame times.
2. **Instant Hot Recompilation**:
   - Ube compiles directly to WebAssembly bytecode without multi-minute LLVM backend overhead.
3. **Safe Memory Boundaries**:
   - The game logic runs inside an isolated WASI 0.2 Component sandbox. Stale entity lookups or logic flaws cannot crash the operating system process.
