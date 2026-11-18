# Example — Flappy Groot

## Core Concept: Complete Arcade Game Loop with Entity Pools and Regional Arenas

A full port of the legacy Flappy Bird sample into pure, native Ube WebAssembly.

### Key Principles

1. **State Machine & Kinematics**:
   - Updates player flap velocity, gravity acceleration, and bounds checking every fixed timestep (`dt = 0.016667`).
2. **Generational Obstacle Pool (`Pool[Pipe]`)**:
   - Pipes spawn dynamically and recycle automatically without creating heap garbage.
3. **Regional Frame Scratch**:
   - Frame rendering runs within `arena { ... }`, maintaining a 0-byte persistent memory delta between game ticks.
