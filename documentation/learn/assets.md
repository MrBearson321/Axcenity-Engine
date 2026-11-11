---
description: Importing models, textures, skies, sounds and fonts, the Assets panel, the Toolbox of free online assets, prefabs, global assets, packages and Asset Check.
icon: download
samples: [hello-mesh, hello-pbr]
---

# Assets: import and manage

Assets are the files a game uses: models, textures, skies, sounds, fonts, animations, materials and prefabs. They live in the project's `assets/` folder, so you can also copy files in with your file manager; the editor notices within a couple of seconds.

## Importing

**File > Import…** (or **Import…** in the Assets panel or on the Model tab) opens one file picker for every kind. Pick several files at once if you like.

| Kind | Formats |
|---|---|
| Models | glTF (`.gltf`, `.glb`) and FBX (`.fbx`) |
| Textures | PNG, JPEG, TGA, BMP |
| Skies | EXR and HDR images (2:1), or PNG and JPEG |
| Sounds | WAV, FLAC, Ogg Vorbis, MP3 |
| Fonts | TTF, OTF |
| Colour grading | `.cube` lookup tables |
| Packages | `.axpkg` (assets exported from another project) |

A model with several objects, textures or animation opens the **Import dialog** first. It previews the model, lists its objects (bring them in as one MeshPart, separate MeshParts, or a Model of them), its materials and texture maps, and its animation takes. A sentence above **Import** says exactly what will be made and where. Imported models are placed at the camera's focus point, at their size in the file (1 metre is 1 stud).

Textures are compressed for the GPU the first time they're used and kept in the project's `.axcenity/` folder, so later opens are fast.

## The Assets panel

The **Assets** panel (beside Output at the bottom) shows the project's assets as tiles, with chips to filter by kind (Models, Prefabs, Images, Textures, Audio, Animations, Skies, Global and more) and a search box. Drag a tile into the Scene to place it: a model is placed where you drop it, a texture or material paints the part under the mouse, a sky becomes the sky. An imported model and its textures and clips show as one folder tile.

Right-click a tile for **Find References** (what uses it), **Rename or Move…** (updates everything that uses it), **Delete…**, **Global asset** and more.

## The Toolbox: free assets online

The **Toolbox** tab (the Assets panel's Toolbox button, or **Ctrl+K**) searches free libraries online, **ambientCG**, **Poly Haven** and **Openverse**, for **Textures**, **Skies**, **Models** and **Sounds**. Get one and it's downloaded once to your computer and copied into the project, with where it came from and its licence remembered. **Favourites** and **Recent** work offline.

- For **Textures** and **Skies**, the source menu's **All sources** (the default) searches every library at once. Each card names its source, and chips above the results show each source's count and narrow the grid to one. If one library is down, the others' results still show.
- The **Sounds** tab shows a licence notice: many sounds aren't cleared for commercial games unless their own licence says so. Each card shows its own licence.
- **Packs ▸ Import plugin…** installs a shared editor plugin (see [Editor interface](editor-interface.md#plugins)).

### Downloaded and imported assets

The **Downloaded / Imported Assets** tab (the Toolbox's view menu, **View > Panels** or **Ctrl+K**) lists every asset that came from outside the editor, whether you downloaded it with the Toolbox, imported it into this project, or both, one row each. Rows are grouped into Models, Textures, Materials, HDRIs, Sounds, Animations and Other, with badges for **Downloaded**, **Imported**, **In project** and **Removed from project**. Filter them by kind, source or a search. Click a row for its source page, authors, licence in plain words, **Copy credit line**, its files, and **Remove download**, which deletes the library's copy (projects keep their own).

Build Game writes a `CREDITS.txt` with the credit lines those licences ask for, and warns about anything not cleared for a commercial game.

## Prefabs

Select parts, models or UI and choose **Save prefab** (Assets panel) to save them as a reusable `.prefab.json`. Placing a prefab makes a **linked copy**: change the prefab and every copy follows. On a copy, **Apply** sends your changes to the prefab, **Revert** takes them back, and **Unpack** makes it an ordinary object again.

## Sharing between projects

- **Global assets**: tick **Global asset** on a tile to copy it into a library every project on this computer can see (the **Global** chip). Using a global asset copies it into the project first.
- **Packages**: right-click an object in the Explorer and choose **Export Package…** to save it, with every asset it uses, as a `.axpkg` file. Import that file in another project.

## Asset Check

**File > Check Assets** lists missing files, textures that are too big, unused assets and other problems, with a **Fix** for many of them.

> **Not supported yet.** Photoshop (`.psd`), DDS and KTX2 textures, USD and OBJ models, and animation in glTF files. Use FBX for animated models.
