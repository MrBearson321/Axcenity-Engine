# Character Studio

Character Studio is a document tab beside the Scene (View > Character Studio, the ribbon's Panels menu, or Ctrl+K "Character Studio"). It shows the selected character's face in a preview of its own, with the **Face** workspace (facial animation F6, decision D9 in [research/facial-animation.md](../research/facial-animation.md)). The default-humanoid work adds its body and customization workspaces to the same tab.

Code: `crates/editor/src/character_studio.rs` (state, preview, the tab) with `character_studio/rig.rs`, `expressions.rs` and `lip_sync.rs` (one per section); the editor's side, which applies what the tab asks for, is `app/character_studio.rs`.

## What it shows

- It follows the selection: a FaceControls in a MeshPart, a MeshPart (with its FaceControls if it has one), or a Model or folder with a FaceControls in a MeshPart inside (`face_target`). With something else selected it keeps the last character.
- The preview draws a copy of the MeshPart alone, at the origin and unturned, posed at rest with its face set by `anim::pose_face` (blend shape weights and eye and jaw turns through the matched profile). The camera looks at the head bone (`anim::face_point`; a model without bones is framed on its top) from the way the body faces. Drag to orbit, scroll to zoom, double-click to frame again.
- What the face shows depends on the section: the FaceControls' own values (Rig), the expression (Expressions), or the visemes at the play head (Lip sync).

## Rig

- **Profile**: the FaceControls' `Profile` (Best fit, a built-in one, or one of the project's `*.face.json`), set as one undo step. The line under it names the profile matched and how many of the 48 controls move something on this model.
- **Check**: what doesn't fit (`anim::check_face`), errors first; after six, the rest fold.
- **Controls**: the FaceControls' 48 controls as sliders, grouped (eyes, eyebrows, mouth and lips, cheeks and nose, jaw and tongue), with a filter. A slider is an edit like Properties' (one undo step per drag). Controls that move nothing on the model are greyed. Without a FaceControls the sliders are off; expressions and lip syncs still preview.
- **Key in Animation** selects the FaceControls and opens the Animation tab, where its controls are rows to key in a clip (F4b). That is the face's timeline: keys, curves and playback are the Animation tab's.
- **Profile mapping** (folded at first): Edit mapping copies the matched profile, keeping only the shapes the model has. Each of the model's blend shapes can be given to one control with a weight (`map_shape`: a shape belongs to one control at a time). The Jaw, LeftEye and RightEye bones can be found by name, set to none, or named; the eye and jaw limits are in degrees. Save profile writes `assets/faces/<name>.face.json` (one undo step) and makes it the FaceControls' `Profile` (another). The preview and the check follow the draft while it's edited.

## Expressions

- The project's expressions, then the built-in ones it doesn't replace (`anim::expression_names`); built-in ones are greyed, and saving one makes the project's own copy.
- **Preview at** scales it; **Blend with** another expression mixes the two by **Blend amount** (preview only).
- The expression's own intensity, transition and the controls it sets (add, change, take out). Save expression writes `assets/faces/expressions/<name>.expression.json` (one undo step); a game plays it with `face:SetExpression("<name>")`.

## Lip sync

- Open any `*.lipsync.json` in the project (hidden folders and `target`, `build` and `node_modules` are skipped; the arrows list the files again).
- **Make from** makes one beside a sound (`.ogg`, `.wav`, `.mp3`, `.flac`: from how it sounds) or phoneme timings (`.lab`, `.tsv`, `.txt`: `start end phoneme` per line), as the Console's `lipsync` does, saves it (one undo step) and opens it.
- The timeline shows the lip sync's sound as a waveform over its length, and its visemes as blocks, each until the next. Click the waveform to move the play head; click a block to pick it, drag it to retime it (kept between its neighbours, `retime`); double-click to add a viseme there (`add_key`). The picked viseme's row changes which viseme it is, its width (weight) and its time, or deletes it; Add at play head adds one.
- Play plays the visemes on the face through the FaceControls' `VisemeMap`, `LipSyncSmoothing` and `LipSyncAnticipation` (or the defaults), as a game would. The sound doesn't play in the tab.
- Length changes the lip sync's duration. Save writes it (one undo step); "Not saved" shows until then.

## Tests

`character_studio/tests.rs` (what a selection shows, mapping shapes, retiming and adding visemes, the waveform, lip sync and expression previews, file names and listing) and `app/character_studio.rs` (the tab on Remy in hello-character: sliders with undo, saving an expression, a profile and a lip sync, Make from a WAV, Key in Animation). `AXCENITY_SNAPSHOTS=<folder>` saves each section, preview drawn, as `character-studio-*.png`. `anim`'s `face_tests.rs` covers `pose_face`, `face_point`, `FaceRig::moves` and `CompiledVisemes::apply`.
