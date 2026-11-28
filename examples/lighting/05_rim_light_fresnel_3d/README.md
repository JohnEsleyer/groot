# Lighting 05 — View-Dependent Rim Lighting & Fresnel Effect

## Core Concept
Demonstrates how to create holographic edge highlights and Fresnel glows by testing each entity's outward-facing normal against the camera's view vector:

$$\text{rim} = (1.0 - \max(0, \vec{N} \cdot \vec{V}))^3$$

### Visual Characteristics
- Faces pointing directly at the camera display their muted base tone.
- Glancing edges perpendicular to the line of sight glow with intense neon turquoise and magenta, giving geometric solids an illuminated, high-tech holographic silhouette.
