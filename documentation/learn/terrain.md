---
description: Sculpt, paint, generate and erode landscapes with the Terrain Studio, and paint grass, plants and trees.
icon: mountains
samples: [hello-part, hello-caves]
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
| | Holes | Cut holes for cave mouths and tunnels; hold **Shift** to fill them |
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

## Scatter rules

Instead of painting by hand, a **scatter rule** (a ScatterRule object inside the terrain) covers the whole terrain for you. In the Terrain Studio, open **Scatter Rules** and pick **Add rule**: the **Forest set** adds trees, undergrowth and grass tufts at once, or add one preset. Select a rule to set how far apart things stand, how many, the heights and slopes they grow on, the material layer they need, how much they gather into patches, and how far they keep from what rules above them placed. With a rule selected, click plants, props or models in the Assets panel to add or remove them. The layout follows the ground as you sculpt and paint, and the same seed gives the same layout every time. Rules keep off anchored parts standing on the ground (a house, a fence, a rock you placed) unless you turn **Avoid Parts** off, and **Align To Slope** and **Sink Depth** lean what they place with the hillside and sink it into the ground. It isn't saved: it's made again when the world opens. **Bake** turns what the rules placed into painted foliage, so you can brush and erase it by hand.

## Biomes

A **biome** (a Biome object inside the terrain) is an area such as a forest, a meadow or alpine turf, with its own ground material and its own scatter rules. In the Terrain Studio, open **Biomes** and pick **Add biome**: each preset brings its ground layer and its rules. A biome covers ground by itself between its heights, below its steepest slope and between its driest and wettest ground, with soft, ragged edges; where biomes overlap, the lower one in the list wins. Select a biome and click **Paint this biome** to spread it by hand (Shift erases back to the automatic biomes). With a biome selected, **Add rule** adds the rule inside it, so it only places there.

## World presets

A **world preset** sets up a whole landscape at once: its shape, materials, biomes, scatter rules and water. Pick the **Generate** tool, choose one under **World Presets** (Temperate Forest, Alpine, Desert, Tropical Island, Tundra or Meadow) and click **Generate with it**; **Apply** keeps the result, all as one undo step. **Only its materials and rules** keeps your terrain's shape. To reuse your own setup, type a name and click **Save as preset**: it's saved in the project's `presets` folder and listed with the others.

## Roads and parts into terrain

- A **SplineMesh** lays a road or fence along a spline, and **Flatten Terrain Along** levels the ground under it. See the **hello-roads** sample.
- A **Terrain Path** (Insert ▸ Terrain Path, into the selected spline) makes the spline a road on the terrain. Scatter rules keep off it straight away, wherever you drag its points. In its Properties, set its **Width**, the **Layer** to paint it with and how far it blends in (**Falloff**), then click **Apply to Terrain** to flatten and paint the ground under it. **Route** lays the spline's points over the terrain from its first point to its last without climbing steeper than **Max Grade**, so the road winds round or up a hill. Both are one undo step each.
- **Solid Modeling ▸ To Terrain** turns a selected part into ground: the terrain under it is raised to its top and painted.
- **Solid Modeling ▸ Conform to Terrain** sets the selected parts or models down on the ground (or up out of it), and **Blend into Terrain** levels the ground under them to their bottom and paints it with the Terrain Studio's Paint layer, fading into the slope around. Each is one undo step.

## Caves and cliffs

A terrain is one surface, so it can't bend back over itself. To make a cave, cut a hole in it and build the cave from parts:

- Pick the **Holes** tool and paint over the spot: the ground there disappears, so you see, walk and fall through it. Hold **Shift** to fill it back in.
- Build the cave under the hole from parts: a floor, walls and a ceiling. Rock parts round the opening hide its edges, and scatter rules keep clear of them.
- Under **Scatter Rules**, **Add rule ▸ Cliff Rocks** stands rock faces on every slope steeper than 38 degrees, and **Boulders** scatters rocks over gentler slopes. The **Alpine** world preset includes cliff rocks.

See the **hello-caves** sample: walk into the hill.

## Buildings

- **Insert ▸ Building** puts a building in front of the camera. Its box is its footprint: drag, turn and stretch it with the Move, Rotate and Scale handles, and it's built again as you go. Inserted into a selected spline, the spline's points are its corners instead, for L-shaped or angled buildings.
- In its Properties, set **Floors**, **Floor Height**, the **Roof Style** (Flat or Gable), **Window Chance** and the colors. Change the **Seed** for another arrangement of windows; the same seed always gives the same building. Name your own models in **Wall Model**, **Window Model**, **Door Model** and **Corner Model** to build with them instead of plain parts.
- Its parts aren't saved: they're made again from its settings. Click **Bake** to keep them as an ordinary Model you can edit by hand. Scatter rules keep trees out of it, and **Blend into Terrain** levels the ground under it. See the **hello-buildings** sample.

## How it's saved

A terrain's data is a folder of images and JSON (`heights.png`, paint maps, `foliage.json` and `holes.png`) under `assets/terrain`, named by the Terrain's **DataFolder** property.
