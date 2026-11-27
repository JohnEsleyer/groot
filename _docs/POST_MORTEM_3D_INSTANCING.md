# Post-Mortem: Corrupted 3D Instanced Cubes

**Date:** 2026-11-27
**Component:** Groot 3D pipeline (`host/src/gpu.rs`, `pkgs/groot/platform3d/platform3d.ube`)
**Severity:** High — the 3D demo was visually unusable
**Status:** Resolved and verified

---

## 1. Summary

The `06_cubes_3d` example rendered as broken, abstract geometric shards with a single
random blue spike instead of a grid of lit, undulating cubes. Two independent defects
combined to produce the corruption:

1. The host vertex shader transposed a **column-major** model matrix, corrupting the
   homogeneous `w` coordinate and projecting geometry across the screen.
2. The host guessed the instance stride from the total float count. A 16-float matrix
   payload from 100 cubes (1,600 floats) was misidentified as 20-float
   matrix-plus-tint records, shifting every subsequent matrix and turning a column into
   a pure-blue tint.

A third, non-structural defect caused flat lighting because the fragment shader ignored
the directional light color.

All three are fixed. The 3D demo now renders the intended 100 instanced cubes orbiting
under the camera above the ground plane.

---

## 2. Symptoms

- Cube faces appeared as disconnected, stretched triangles and shards.
- Geometry jumped between clip-space extremes instead of forming stable cubes.
- One large, pure-blue spike appeared at random.
- Lighting was flat and did not respond correctly to the configured light color.
- The ground plane, submitted as a single instance, was less visibly affected than the
  cube grid.

---

## 3. Relevant Architecture

Groot's 3D path is split across three layers:

| Layer | File | Responsibility |
| :--- | :--- | :--- |
| Guest math | `pkgs/groot/math/math.ube` | Column-major `Mat4` values |
| Guest draw API | `pkgs/groot/platform3d/platform3d.ube` | Packs instance data and calls `ube:graphics/gpu3d` |
| Host GPU | `host/src/gpu.rs` | Uploads instance buffers and runs WGSL shaders |

Each 3D instance is expected to occupy **20 floats**: 16 floats for a column-major
model matrix plus 4 floats for an RGBA tint.

```wgsl
struct InstanceInput {
    @location(3) model_matrix_0: vec4<f32>,
    @location(4) model_matrix_1: vec4<f32>,
    @location(5) model_matrix_2: vec4<f32>,
    @location(6) model_matrix_3: vec4<f32>,
    @location(7) tint_color: vec4<f32>,
};
```

`Mat4` is stored column-major on the guest side:

```ube
pub fn mat4_translate(x: f32, y: f32, z: f32) -> Mat4 {
    return Mat4{
        m: [16]f32{
            1.0, 0.0, 0.0, 0.0,  // column 0
            0.0, 1.0, 0.0, 0.0,  // column 1
            0.0, 0.0, 1.0, 0.0,  // column 2
            x,   y,   z,   1.0   // column 3: translation
        }
    }
}
```

---

## 4. Root Cause 1 — Transposing an Already Column-Major Matrix

The vertex shader constructed the model matrix like this:

```wgsl
let model = transpose(mat4x4<f32>(
    instance.model_matrix_0,
    instance.model_matrix_1,
    instance.model_matrix_2,
    instance.model_matrix_3,
));
```

WGSL's `mat4x4<f32>(c0, c1, c2, c3)` constructor already takes **column vectors**.
Transposing the matrix moved the translation column into the matrix's fourth row:

- Rows 0–2 no longer contained translation, so the model's `xyz` transform dropped the
  translation.
- The homogeneous coordinate became:

  ```text
  world_pos.w = x * vx + y * vy + z * vz + 1.0
  ```

  instead of `1.0`.
- Perspective division by that warped `w` sent vertices toward opposite clip planes and
  stretched polygons diagonally across the viewport.

This explains the shattered, screen-spanning geometry rather than a simple offset.

**Fix:** construct the matrix directly from the four column vectors:

```wgsl
let model = mat4x4<f32>(
    instance.model_matrix_0,
    instance.model_matrix_1,
    instance.model_matrix_2,
    instance.model_matrix_3,
);
```

---

## 5. Root Cause 2 — Ambiguous 16-vs-20-Float Instance Stride

The guest `draw_instances` path originally emitted only 16 floats per cube (the matrix)
and left the tint to the host. The host tried to infer the layout from the total length:

```rust
// Before: ambiguous length-based layout detection
let (instance_count, packed_data) = if instance_data.len() % 20 == 0 {
    ((instance_data.len() / 20) as u32, instance_data.to_vec())
} else if instance_data.len() % 16 == 0 {
    // expand each 16-float matrix with a white tint
} else {
    return;
};
```

This heuristic is not reliable:

- 16-float records: `16 * N` floats.
- 20-float records: `20 * M` floats.
- `lcm(16, 20) = 80`, so any `N` that is a multiple of 5 produces a total length that is
  also divisible by 20.

`06_cubes_3d` submits exactly **100 cubes**:

```text
100 * 16 = 1600 floats
1600 % 20 == 0
```

The host therefore took the 20-float branch and interpreted the payload as 80 records:

- Floats 16–19, the first column of cube 1's matrix, became cube 0's RGBA tint.
- Cube 1's matrix started at offset 20, scrambling its columns.
- The offset error accumulated for every subsequent cube.
- Columns containing values such as `[0.0, 0.0, 1.0, 0.0]` were consumed as tints,
  producing the random pure-blue shard.

The single ground-plane instance (`16` floats, not divisible by `20`) took the correct
branch, which is why the ground was less obviously corrupted than the cube grid.

**Fix:** make the guest always emit a canonical 20-float record:

```ube
pub fn draw_instances(mesh_id: i32, view transforms: []math::Mat4) {
    let mut inst_floats = []f32{}
    let mut t_idx = 0
    while t_idx < transforms.len {
        let mut m_idx = 0
        while m_idx < 16 {
            inst_floats.push(transforms[t_idx].at(m_idx))
            m_idx += 1
        }
        // Default white tint
        inst_floats.push(1.0)
        inst_floats.push(1.0)
        inst_floats.push(1.0)
        inst_floats.push(1.0)
        t_idx += 1
    }
    host_gpu3d_draw_instances(mesh_id, inst_floats.ptr, inst_floats.len)
}
```

A new `draw_instances_tinted` API supplies per-instance colors using the same 20-float
layout.

### Follow-up issue: Ube `view` restrictions

The first implementation read tint fields directly from the viewed slice:

```ube
inst_floats.push(tints[t_idx].r) // rejected
```

Ube views are second-class, so a viewed element cannot be unpacked field-by-field into
another slice. The fix introduces a helper that returns a copy-type fixed array:

```ube
fn color_floats(view c: color::Color) -> [4]f32 {
    return [4]f32{ c.r, c.g, c.b, c.a }
}
```

The caller forwards the viewed element to the helper and pushes the copied components.
This preserves the read-only `view` contract and compiles cleanly.

---

## 6. Root Cause 3 — Fragment Shader Ignored Light Color

The fragment shader used only the light direction:

```wgsl
let final_light = ambient + diff * 0.85;
return vec4<f32>(in.tint.rgb * final_light, in.tint.a);
```

As a result, `set_directional_light`'s color parameter had no effect and faces were
shaded with a fixed white response.

**Fix:** apply the light color and keep the tint in the lit term:

```wgsl
let ambient = 0.2;
let final_light = in.tint.rgb * (ambient + diff * 0.8 * light.color.rgb);
return vec4<f32>(final_light, in.tint.a);
```

---

## 7. Verification

- `cargo check` in `_groot/host` passes.
- All eight examples compile to WASI 0.2 components:

  ```bash
  cd _groot
  for ex in examples/*/main.ube; do
      ube build "$ex" --target=wasi2 -o /tmp/"$(basename "$(dirname "$ex")")".wasm
  done
  ```

- `06_cubes_3d` was rebuilt and run through `groot-host`; the cubes render correctly as a
  lit, instanced grid with the orbiting camera.

The stride and lighting fixes were committed as:

```text
8aa0f80 fix(3d): correct instance stride and use light color in shading
```

---

## 8. Lessons & Preventive Actions

1. **Do not infer ABI layout from total byte length.** Use an explicit record format or
   a versioned contract. A length that is divisible by both candidate strides is
   inevitable and silent.
2. **Keep the canonical instance stride in one place.** The host already defines
   `FLOATS_PER_INSTANCE3D = 20`; guest packing and host validation should both refer to
   that contract.
3. **Add boundary tests for instance counts.** In particular, test counts divisible by
   `5` (the ambiguous 16-float case) and counts of `1`, `2`, and `100`.
4. **Test matrix conventions at the boundary.** A tiny GPU test that verifies
   `translate(x, y, z)` moves a point by exactly `(x, y, z)` would have caught the
   transpose immediately.
5. **Remove or harden the remaining 16-float fallback.** `draw_instance_floats` can still
   pass raw data whose length is ambiguous. Prefer a typed call or require an explicit
   stride/layout argument.

---

## 9. Final State

- `host/src/gpu.rs`: correct column-major model construction; light color applied.
- `pkgs/groot/platform3d/platform3d.ube`: canonical 20-float instance packing;
  `draw_instances_tinted`; `view`-safe color unpacking.
- `examples/06_cubes_3d`: renders as a 100-cube instanced wave around a ground plane
  with a smoothly orbiting camera.
