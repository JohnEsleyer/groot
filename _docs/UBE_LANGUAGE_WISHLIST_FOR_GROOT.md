# Language Wishlist: Ube Features for Game Engines & Groot

This document outlines key language-level and compiler features that would significantly elevate **Groot** (and real-time systems/game development in general) when authoring high-performance, ergonomic, and zero-GC engines in **Ube**.

---

## 1. Ergonomic In-Place Mutable Borrowing (`&mut` / `loan mut`)

### Current Limitation
In Ube, `view` parameters are strictly read-only loans. When modifying an aggregate, a struct field, or a slice element, developers frequently encounter affine ownership restrictions:
- You cannot pass a mutable borrow of a subfield to a helper function.
- Slices of structs cannot be easily updated in-place without either using full field reassignment (`slice[i] = Struct{ ... }`) or extracting values to local variables.
- Modifying nested components inside entities stored in a `Pool[T]` requires replacing the entire snapshot or manually copying fields back and forth.

### Proposed Feature
Add second-class **mutable view loans** (`mut view` or `loan mut`):
```ube
fn update_physics(mut view body: RigidBody, dt: f32) {
    body.velocity.y += 980.0 * dt
    body.position.x += body.velocity.x * dt
    body.position.y += body.velocity.y * dt
}

// In-place invocation directly on collection elements:
update_physics(mut view bodies[i], dt)
```
- **Safety Rule**: Follows affine exclusivity—only one active `mut view` can exist for a given memory region at a time, completely free of runtime locks or lifetime annotations.

---

## 2. Operator Overloading for Value Math Types

### Current Limitation
Game development is heavily saturated with linear algebra operations: `Vec2`, `Vec3`, `Mat4`, `Color`, and `Rect`. Currently in Ube, every simple arithmetic operation requires verbose, chained method or function calls:
```ube
// Current Ube:
let n = hit.normal
let relative = (bodies[1].vx - bodies[0].vx) * n.x + (bodies[1].vy - bodies[0].vy) * n.y
let b0vx = bodies[0].vx - relative * n.x
let b0vy = bodies[0].vy - relative * n.y

let offset = cam.position.sub(target_pos).scale(cam.zoom)
```
This degrades readability, increases token cost for LLMs, and makes complex mathematical physics engines (GJK, EPA, constraint solvers) tedious to maintain.

### Proposed Feature
Deterministic, non-allocating operator syntax for Copy types:
```ube
pub struct Vec2 { pub x: f32, pub y: f32 }

pub fn (a Vec2) + (b Vec2) -> Vec2 {
    return Vec2{ x: a.x + b.x, y: a.y + b.y }
}

pub fn (a Vec2) * (s: f32) -> Vec2 {
    return Vec2{ x: a.x * s, y: a.y * s }
}

// Resulting game code:
let relative = (b1.velocity - b0.velocity).dot(n)
b0.velocity -= n * relative
```

---

## 3. Native SIMD / Vector Intrinsics (`v128` Wasm Vector Primitives)

### Current Limitation
WebAssembly has full, high-throughput support for 128-bit vector operations (`wasm32-simd128`), which native runtimes like Wasmtime lower directly into AVX2, AVX-512, and ARM NEON hardware registers.
Currently in Ube:
- Vector math loops (`01_bunnymark`, particle physics, sprite vertex transformations) run strictly scalar `f32` arithmetic.
- Transforming 4 floats in `SpriteBatcher` or camera projections takes 4 distinct instructions and separate load/stores instead of a single vector instruction.

### Proposed Feature
Expose native 128-bit SIMD primitives in `std/simd` or as primitive types (`f32x4`, `i32x4`):
```ube
import "std/simd" as simd

let positions_x: simd::f32x4 = simd::f32x4_load(ptr)
let velocities_x: simd::f32x4 = simd::f32x4_load(v_ptr)
let dt_vec = simd::f32x4_splat(dt)

// Emits f32x4.add and f32x4.mul in 1 CPU cycle
let new_positions = positions_x + velocities_x * dt_vec
```
This would enable Groot to easily handle **50,000+ sprites at 120 FPS** in Bunnymark tests.

---

## 4. Compile-Time Fixed-Capacity & Stack Arena Allocators

### Current Limitation
Currently, dynamic slices (`[]T`) are Tier-2 allocations allocated in linear memory via bump pointer (`global.get 0`). If an array or batch is cleared and rebuilt every frame inside an `arena`, capacity expansions or resizing can create orphaned buffers. Furthermore, fixed-size arrays (`[N]T`) cannot track a dynamic `len`.

### Proposed Feature
**Fixed-capacity bounded slices** (`BoundedSlice[T, N]` or `[N]T with len`):
```ube
// Stored completely inline on Tier-1 stack or inside struct without bump allocation:
struct ParticleEmitter {
    particles: [256]Particle,
    count: i32
}
```
With convenience syntax:
```ube
let mut local_batch: [512]Vertex
local_batch.push(v) // bounded by static capacity 512, zero heap/arena overhead
```

---

## 5. First-Class Host Function Pointers & Event Callbacks

### Current Limitation
In WASI 0.2 and component models, communication is guest-driven. Groot currently suffers an architectural split where:
- The guest owns a synchronous `while is_running` loop.
- The host `winit` event loop cannot pump events while the guest loop is running on the same thread without explicit polling syscalls on every frame.

### Proposed Feature
Support **guest export hooks** or **yieldable execution steps**:
```ube
// Guest registers frame update callback
pub fn on_update(dt: f32) {
    // Host calls this per winit RedrawRequested / AboutToWait event
}

pub fn on_input(event: InputEvent) {
    // Host pushes OS events directly into guest
}
```
Or a native `yield` keyword that suspends guest execution and yields control back to the host runtime between frames:
```ube
while game.is_running {
    game.update(dt)
    game.render()
    yield // Yields frame boundary back to host event loop
}
```

---

## 6. Zero-Cost Enums with Inline Payload Flattening

### Current Limitation
In the current compiler, complex optional structs (such as `Option[Sprite]` or `Sprite?` inside an aggregate) trigger memory alignment or pointer offset issues in `Pool[T]`. This forced Groot to manually flatten `Entity` into `has_sprite: bool` and a dummy `sprite: Sprite`.

### Proposed Feature
Compiler-automated **discriminated union layout optimization**:
- Automatically pack `T?` as `{ has_value: bool, value: T }` without heap indirection.
- Allow pattern matching with zero memory allocations or pointer offsets:
```ube
if let s = entity.sprite {
    // s is borrowed directly from the entity payload, zero copying
}
```

---

## 7. Struct Field Default Values & Struct Update Syntax

### Current Limitation
Constructing large configuration structs or math primitives requires specifying every single field:
```ube
pub struct Transform {
    pub position: Vec2,
    pub rotation: f32,
    pub scale: Vec2
}

// Every instantiation must type every field:
let t = Transform{
    position: vec2(0.0, 0.0),
    rotation: 0.0,
    scale: vec2(1.0, 1.0)
}
```

### Proposed Feature
1. **Default values on struct declarations**:
```ube
pub struct Transform {
    pub position: Vec2 = vec2_zero(),
    pub rotation: f32 = 0.0,
    pub scale:    Vec2 = vec2_one()
}

let t = Transform{ position: vec2(10.0, 20.0) } // rotation and scale use defaults
```
2. **Functional struct update syntax**:
```ube
let t2 = Transform{ position: new_pos, ..t }
```

---

## 8. Built-in Testing Assertions (`assert` / `assert_eq`)

### Current Limitation
When running package tests via `ube test`, the compiler does not have a built-in `assert` or `assert_eq` macro/intrinsic. Writing unit tests currently requires manual `if !cond { panic() }` or printing status strings.

### Proposed Feature
Built-in `assert(expr, message)` and `assert_eq(a, b)`:
```ube
fn test_aabb_overlap() {
    let r1 = math::rect(0.0, 0.0, 10.0, 10.0)
    let r2 = math::rect(5.0, 5.0, 10.0, 10.0)
    assert(physics2d::overlaps(r1, r2), "Rectangles should intersect")
}
```
If an assertion fails, the test runner prints the file, line, expected vs actual values, and registers a failed test step cleanly.

---

## Conclusion & Impact Matrix

| Feature | Primary Benefit for Groot | Implementation Complexity |
| :--- | :--- | :---: |
| **Operator Overloading for Math** | Eliminates boilerplate; turns verbose `.sub().scale()` into clean math formulas. | Low |
| **Struct Defaults & Update Syntax** | Clean entity component definitions, scene builders, and config objects. | Low |
| **Built-in `assert` in `ube test`** | Enables robust test-driven development for math & physics suites. | Low |
| **Discriminant Union Flattening** | Safe `Option[T]` inside `Pool[Entity]` without manual boolean flags. | Medium |
| **`mut view` Loan Parameters** | In-place component and slice mutations without copying or indexing bugs. | Medium |
| **Host `yield` / Export Callbacks** | Flawless 60/120 FPS host window event loop synchronization. | Medium |
| **SIMD `v128` Intrinsics** | 4x to 8x throughput leap for sprite batch vertex transformation and physics. | High |
