---
description: Give parts a look with colours, built-in materials, PBR texture maps, material files, the Material tab and material graphs.
icon: palette
samples: [hello-pbr, hello-graphics, hello-material-graph]
---

# Materials and textures

A part's look comes from four layers, each optional, from simplest to most flexible:

1. Its **Color**, **Material**, **Metalness**, **Roughness**, **Transparency** and **Emission** properties.
2. A **SurfaceAppearance** inside it: texture maps (colour, normal, roughness, metalness, ambient occlusion, emission) and tiling.
3. A **material file** (`*.material.json`) named by its **MaterialVariant**: a reusable look many parts share.
4. A **surface shader** or **material graph** in that file, for looks the standard surface can't make.

## Colour and built-in materials

Select parts and use the **Paint** tool (**V**) for colour, or the **Material** tool (**N**) for a material. The built-in materials are Plastic, SmoothPlastic, Wood, WoodPlanks, Marble, Granite, Concrete, Slate, Brick, Fabric, Sand, Metal, Glass, Neon, Ice and Rubber. A material also sets how a part behaves in physics: Metal is heavy, Ice is slippery and Rubber is bouncy (see [Physics and collisions](physics.md)). **Neon** glows.

The Material tool's **Prototype** section has grid textures in six colour families, handy for blocking out a level. Picking one copies its image and material file into your project, so the project stays self-contained.

## Texture maps

Axcenity draws physically based materials (metallic-roughness PBR). A model's own materials and maps come with it when you import it. For a part, insert a **SurfaceAppearance** into it and set its maps in Properties, or drag a texture set from the Assets panel onto the part. Texture files can be PNG, JPEG, TGA or BMP.

Maps are recognised by their file names too (`_BaseColor`, `_Normal`, `_Roughness`, `_ORM` and so on), including channel-packed maps where one image holds several. The editor **cooks** each texture into a compressed copy (BC7, or BC5 for normal maps) with mipmaps the first time it's used, stored per computer in `.axcenity/imported`, so large textures load quickly and use less video memory.

## Material files and the Material tab

- **Save as Material** in a part's Explorer menu saves its look as `assets/materials/<name>.material.json`. Drag a material from the Assets panel onto parts to use it.
- The Material tool's **Custom** section lists your project's materials, makes a **New…** one from colour, normal and roughness images, or **Load…**s one from anywhere (its images come too).
- **Edit material** (a material's right-click menu in Assets) opens the **Material** tab: every value and map, a live preview, presets, and **Builds on**, which makes a material a variant of another so it only stores what differs.
- The Toolbox can download free textures from ambientCG and Poly Haven straight into the project, with their licence recorded. See [Assets: import and manage](assets.md).

## Shading models and special materials

A material file can choose its shading: **Standard**, **Unlit** (flat colour, no lighting) or **Toon** (banded light), add an **outline**, be **translucent** like leaves, or be **water**. The **hello-toon**, **hello-outlines** and **hello-water** samples show each.

## Shaders and material graphs

> **For advanced users.** In the Material tab, **Add shader** gives a material a surface shader written in WGSL, checked as you type with errors shown on their line. The tab's **Graph** view builds the same thing from nodes instead, with no code, and the editor turns the graph into a shader. Both run in built games. The **hello-shader** and **hello-material-graph** samples each have one.

## Decals

A **Decal** inside a part projects an image onto whatever surfaces its box covers, such as a sign or a stain, without changing those surfaces' materials. See the **hello-decals** sample.
