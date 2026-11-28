# Lighting 03 — 2D Dungeon Torch & Dynamic Point Falloff

## Core Concept
Demonstrates real-time 2D radial point-light simulation using CPU vertex tint modulation. A 600-tile dungeon room is bathed in ambient darkness; an interactive player character carries a flickering torch whose illumination smoothly falls off with distance.

### Techniques
1. **Distance-Based Quadratic Attenuation**:
   $$I(d) = \max\left(0, 1.0 - \left(\frac{d}{\text{radius}}\right)^2\right)$$
2. **Organic Flame Flicker**: Modulates torch radius and warmth using multi-frequency harmonic sines.
3. **Interactive Control**: Move the torch bearer using `WASD` or Arrow Keys.
