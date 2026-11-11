---
description: Sculpt, paint, generate and erode landscapes with the Terrain Studio, and paint grass, plants and trees.
icon: mountains
samples: [hello-part]
---

# Terrain tools

A **Terrain** object is a heightmap landscape: smooth hills and valleys, painted with layers of material and covered with painted foliage. You shape it with brushes in the **Terrain Studio**, and parts and characters walk and roll on it like any other ground.

## Open the Terrain Studio

On the **World** tab, the **Terrain** group opens the Terrain Studio panel; **Brush ▾** picks a tool directly. With a brush up, the left mouse button paints in the Scene, **[** and **]** make the brush smaller and bigger, and **Escape** puts it down. Hover a tool for its name, key and what it does.

## Tools

| Key | Tool | Does |
|---|---|---|
| **1** | Raise / Lower | Raise ground; hold **Shift** to lower |
| **2** | Smooth | Even out bumps |
| **3** | Flatten | Level to a height |
| **4** | Sculpt | Push and pull freely |
| **5** | Stamp | Press a shape into the ground |
| **6** | Noise | Add natural roughness |
| **7** | Paint | Paint a material layer |
| **8** | Auto Materials | Paint by height and slope rules: grass on flat ground, rock on cliffs |
| **9** | Generate | Make a whole landscape from settings and a seed, or a preset |
| **0** | Erosion | Weather the landscape with water and gravity |
| | Foliage | Paint plants, props or models |
| | Masks | Limit tools by height or slope |

Generate and Erosion run in the background with a progress bar and Cancel. A seed always gives the same landscape.

## Materials

A terrain is painted with **layers** (TerrainLayer objects inside it), each with its own textures and a rule for where it shows. Open a layer in the Terrain Studio and pick **Use texture set** to fill all its maps from a texture in your project, or **From Toolbox…** to download one.

## Foliage

The **Foliage** tool scatters the selected item from the Assets panel (a built-in plant or prop, or one of your models) as you paint, with a density. Click a prop or model in Assets to choose it, and hold **Shift** to erase it. Foliage can be solid, so parts rest on it, or only visual. Painted plants sway in the Workspace's **GlobalWind** and are drawn only near the camera (grass to about 120 studs, trees to 800), so a large painted forest stays fast.

## Roads and parts into terrain

- A **SplineMesh** lays a road or fence along a spline, and **Flatten Terrain Along** levels the ground under it. See the **hello-roads** sample.
- **Solid Modeling ▸ To Terrain** turns a selected part into ground: the terrain under it is raised to its top and painted.

## How it's saved

A terrain's data is a folder of images and JSON (`heights.png`, paint maps and `foliage.json`) under `assets/terrain`, named by the Terrain's **DataFolder** property.
