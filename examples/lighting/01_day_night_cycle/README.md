# Lighting 01 — 24H Celestial Day/Night Cycle

## Core Concept
Simulates a continuous 24-hour day/night cycle by sweeping a directional sun light along an inclined orbital arc and interpolating ambient light, light hue, light intensity, and sky clear colors across four distinct diurnal phases.

### Techniques
1. **Orbital Light Vector**: The sun direction is calculated via `sin(t)` and `cos(t)` along an inclined plane.
2. **Four-Phase Diurnal Color Grading**:
   - **Midday**: Pure warm white sunlight (`1.0, 0.98, 0.92`), clear sky blue (`0.25, 0.55, 0.90`).
   - **Sunset / Dawn**: Amber/crimson directional light (`1.0, 0.45, 0.15`), dusk purple clear (`0.30, 0.12, 0.25`).
   - **Night / Moonlight**: Soft cool indigo directional light (`0.15, 0.25, 0.45`), deep midnight clear (`0.02, 0.02, 0.05`).
