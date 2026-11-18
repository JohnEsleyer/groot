# Example 05 — Camera and Coordinate Transforms

## Core Concept: Orthographic Projections, Pan, Zoom, and Screen Mapping

Demonstrates how to decouple gameplay coordinates from screen pixel coordinates using `Camera2D`.

### Key Principles

1. **World vs. Screen Coordinates**:
   - `world_to_screen(world_pos)` centers the viewport, adjusts for zoom scale, and translates camera position into window coordinates.
2. **Camera Matrices & Viewports**:
   - Decoupled from native window metrics; viewport transformations easily support letterboxing and multi-resolution rendering.
