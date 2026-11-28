# Lighting 04 — Simulated 3D Point Lights via Instance Attenuation

## Core Concept
Demonstrates how to achieve local 3D point-light illumination with quadratic falloff over an instanced mesh grid (400 pedestals) without writing custom host shaders.

### Techniques
1. **Dynamic Point Orb**: An illuminated light sphere orbits and bounces through 3D space along a 3-axis path.
2. **Instance-Level Light Attenuation**:
   $$A = \frac{1.0}{1.0 + 0.08 \cdot d + 0.015 \cdot d^2}$$
3. **Diffuse $N \cdot L$ Approximation**: Modulates instance tints by both distance and angle towards the moving light source.
