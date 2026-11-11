# The Toolbox in the editor

How the Toolbox should look and work inside the editor (brief 3.3), and where it plugs into the systems the editor already has (brief 3.4). Part of [Axcenity Toolbox in the engine](README.md); the interface it calls is in [provider-interface.md](provider-interface.md). Dated 2026-11-11. A proposal: none of this is built.

The rule throughout: **the Toolbox finds and fetches; the editor's existing paths install and apply.** A texture from the Toolbox becomes the same texture set Get texture makes today, a model goes through the same Import dialog, a sky through the same "Use as the sky?" question. No second asset workflow.

## 1. Where it lives

- A **Toolbox** tab beside **Assets** and **Output** at the bottom of the window (they already share that area through tabs, `docs/editor/assets.md`), so it can also float in its own OS window like other panels. The Assets panel's **Get texture** button becomes **Toolbox** and opens it.
- It follows the panel rules in `crates/editor/CLAUDE.md`: colours from `theme::palette`, controls from `widgets.rs`, and it fits a 260-point column through `responsive.rs` (a list when narrow, cards when wide, like the Assets panel).
- Ctrl+K universal search gets a Toolbox provider (`app/search.rs`, `SearchProvider`), listing library and favourite assets by name.

## 2. Discovering assets

| Control | Behaviour |
|---|---|
| Search box | Searches every enabled source for the chosen kind at once; each source's results arrive on their own and a failing source shows its safe message on its own row, so one source being down doesn't empty the panel |
| Kind tabs | **Textures and materials**, **Skies** (HDRIs), **Models**, **Sounds**: the brief's four first types |
| Source chips | All, ambientCG, Poly Haven, Openverse (sounds); later sources appear here when added |
| Categories | Chips from each source's own categories (Poly Haven's `categories`, ambientCG's categories endpoint), shown for the source in view |
| Sort | Relevance, Popular (the provider's download count), Newest, Name |
| Licence filter | Default: CC0, public domain and CC BY. **Show restricted** reveals NC, ND, SA and unknown licences, marked as blocked for a commercial game and why |
| In library | Only assets already downloaded on this machine; this is also what the panel shows offline |

Each result is a card like Get texture's (preview, name, source, licence badge), and the preview shows larger on hover. Sounds have a play button that streams the provider's preview through the editor's audio, stopping when another plays or the panel closes. Previews are fetched a few at a time, as today, and kept in the library's `previews/` folder, so a second search or a second project shows them at once.

**Attribution.** Every card names its source; Poly Haven results carry "Powered by Poly Haven" (its API terms, 2.5); a card whose licence needs credit says so on its badge.

## 3. The details drawer

Clicking a card opens a drawer (the Toolbox's own library drawer is the model): name, author, source page link, licence in plain words with what it allows and asks (`license.rs`, ported from the Django Toolbox), physical size, resolutions, maps, and each variant with its download size. It also shows whether the asset is **in the library** (which resolutions) and **in this project** (from the project's sidecar `source` blocks), plus **Get**, **Favourite**, **Add to collection** and **Copy credit line**.

**Get** uses the member's defaults (resolution, format, maps; set in the panel's settings, like the Django Toolbox's download defaults) and `variants::choose`, and says where it had to differ ("No 8K here, so 4K, the nearest.").

## 4. Into the project, through existing systems

| Asset | Get installs to | Drag onto the viewport, or double-click | Elsewhere |
|---|---|---|---|
| Texture or material | `assets/textures/<Id>/` and `<Id>.texture.json` (unchanged from Get texture) | Same as dropping a texture set today: fills a Part's SurfaceAppearance, every appearance of a MeshPart, or the top TerrainLayer, as one undo step | **Make material** writes `assets/materials/<Id>.material.json` naming the maps and opens it in the Material tab; Terrain Studio's **Use texture set** menu gets **From Toolbox…**, and a card dropped on a layer fills it |
| Sky (HDRI) | `assets/sky/<id>_<res>.hdr` | Asks "Use *name* as the sky?" (`app/sky.rs`) | Shown under Skies in the Assets panel |
| Model | `assets/models/<id>/` with its glTF's files | Opens the Import dialog on the downloaded file (scale, collision and the other options it has now), then places the MeshPart where the drop marker is | Explorer gets the MeshPart as any import does; the Model Studio can open it |
| Sound | `assets/audio/<id>.<ext>` | A Sound in the part under the mouse, else the Workspace, as dropping a sound file does now | The Sequencer and scripts use it by path or ID as any sound |

Dragging a card works before it is downloaded: the drop downloads (progress in the bottom-right `toast::Busy`, cancellable), installs, then applies. Installing is not an undo step, as downloading isn't today; applying it is one undo step. Every installed file gets an `ax://` ID and a sidecar `source` block, so Asset Check, Find References, Rename or Move…, Export Package and Global assets treat it like any other file, and the Assets panel tooltip shows its source and licence.

**Originals and conversion.** The library keeps the provider's files untouched. The project gets a copy, which the existing cooking (`cook`, BC7 textures) and level-of-detail steps process as they do any project file, so no Toolbox-specific conversion is needed. A future **Optimise** option (smaller resolution on install, ORM packing) would sit in the install step, not in the providers.

## 5. Managing assets

| Feature | Where it's kept | Notes |
|---|---|---|
| Favourites | The library, `metadata/toolbox.json` (per machine, under the library lock) | A star on cards and in the drawer; a **Favourites** view |
| Collections | Same file | Named lists of `provider:id` with an optional chosen variant and folder; **Export collection** writes an `axcenity-pack` manifest and **Import Toolbox pack…** reads one, so collections and the Django site's packs are the same thing |
| Recent | Same file | The last 100 assets viewed or used |
| Download history | The library registry's `downloads` (each resolution, its files, size and time) | A **History** view, with **Remove from library** to reclaim disk space (projects keep their copies) |
| In this project | The project's sidecar `source` blocks | A **This project** view, and the project's credits |
| Licences and credits | `license.rs`, the sidecars | A **Credits** view for the project; Build Game writes `CREDITS.txt` |

## 6. Not in scope now

- **Publishing your own assets** to a shared catalogue (Roblox's Creator Store). It needs accounts, hosting and moderation, which the development policy defers; the Django site's public packs are the nearest thing, by manifest file.
- **Accounts in the editor.** Sources that need a key (Sketchfab, Freesound) read it from the environment; a credential store waits until a source in the editor needs one.
- **Every source the research found.** Each later source (Kenney, Sketchfab, Smithsonian 3D, Wikimedia Commons, Freesound) is judged on its own: licence clarity, API terms, and whether its assets are game-ready.

## 7. Order

The panel arrives in the migration plan's steps (see [provider-interface.md](provider-interface.md), section 8): Get texture keeps working through steps 1 to 3; the Toolbox panel replaces it in step 4 with textures and skies; models follow in step 6 and sounds in step 7; favourites, collections, recents and history come with the panel's library views in step 5.
