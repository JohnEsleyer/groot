# Groot Engine — Lighting Laboratory & Scenarios

This directory contains standalone lighting experiment projects demonstrating various lighting models, techniques, and visual pipelines in Groot using **Ube** and **WASI 0.2**.

---

## Projects in this Suite

| Project | Mode | Scenario & Techniques Demonstrated |
| :--- | :---: | :--- |
| **[`01_day_night_cycle`](01_day_night_cycle/README.md)** | 3D | Celestial orbital mechanics, dynamic sun vector, multi-phase skybox/clear color transitions (dawn/noon/dusk/midnight), and moon fill light. |
| **[`02_colored_studio_rig`](02_colored_studio_rig/README.md)** | 3D | 3-point studio lighting simulation, complementary and high-contrast color harmonics, and directional light color absorption against tinted materials. |
| **[`03_2d_dungeon_torch`](03_2d_dungeon_torch/README.md)** | 2D | Real-time 2D radial point lighting, procedural flame flicker, quadratic distance attenuation across 600 tiles, and interactive WASD torch movement. |
| **[`04_point_light_grid_3d`](04_point_light_grid_3d/README.md)** | 3D | CPU-side point-light attenuation for dense 3D instance meshes: calculates distance falloff and $N \cdot L$ diffuse terms into per-instance tints on 400 pedestals. |
| **[`05_rim_light_fresnel_3d`](05_rim_light_fresnel_3d/README.md)** | 3D | View-dependent rim lighting and Fresnel effect: calculates view angle $(\vec{N} \cdot \vec{V})$ to generate glowing silhouette halos and metallic sheens on orbiting geometry. |

---

## How to Run Any Lighting Example

```bash
# Using the groot CLI from any project directory:
cd examples/lighting/01_day_night_cycle
groot start .

# Or compile and launch with groot-host:
ube build main.ube --target=wasi2 -o game.wasm
cargo run --manifest-path ../../../host/Cargo.toml -- game.wasm
```
