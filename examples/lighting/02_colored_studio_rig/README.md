# Lighting 02 — Colored Studio Rig & Multi-Hue Interaction

## Core Concept
Demonstrates how materials and instance tints absorb and reflect directional light colors. The scene renders a central geometric sculpture surrounded by rings of faceted cubes under an orbiting key light that cycles through saturated complementary hues (Amber $\leftrightarrow$ Violet $\leftrightarrow$ Emerald).

### Formula Under Inspection
$$\text{LitColor} = \text{tint} \times (\text{ambient} + (N \cdot L) \times 0.78 \times \text{LightColor})$$
Objects with matching color channels glow intensely, while complementary surfaces naturally fall into dark, stylized contrast.
