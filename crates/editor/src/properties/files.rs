//! Controls for properties that name a file: texture maps, images, colour
//! grading tables, the sky, animation clips, controllers, behavior trees,
//! script sources and sounds. Each shows the file and buttons to pick
//! another or clear it.

use super::*;

/// The name of the file `reference` names, with its project path on hover.
/// A reference by asset ID shows the file with that ID, or says no file has
/// it (its sidecar may be gone; Asset Check lists it).
pub(super) fn file_label(ui: &mut egui::Ui, assets: &Assets, reference: &str) {
    let Some(path) = assets.asset_path(reference) else {
        ui.weak("No file has this ID").on_hover_text(format!(
            "No file in the project has the asset ID {reference}: its .import.json \
             sidecar may be gone. Asset Check lists it."
        ));
        return;
    };
    let hover = if path == reference {
        path.clone()
    } else {
        format!("{path}\n(by its asset ID {reference}, so it follows the file when it moves)")
    };
    let file = path.rsplit('/').next().unwrap_or(&path);
    ui.add(egui::Label::new(file).truncate())
        .on_hover_text(hover);
}

/// A texture map's file, with buttons to pick another and to clear it.
/// Typing isn't offered: each half-typed path would be loaded, and fail.
pub(super) fn map_control(
    ui: &mut egui::Ui,
    name: &'static str,
    path: &str,
    assets: &Assets,
) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose a texture file (PNG, JPEG, TGA or BMP)")
            .clicked()
        {
            edit = Some(Edit::PickTexture(name));
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Use no map")
            .clicked()
        {
            edit = Some(Edit::Property(name, Value::String(String::new())));
        }
        file_label(ui, assets, path);
    });
    edit
}

/// A UI element's picture: typed as a project path, uploaded from this
/// computer (copied into `assets/images`), or dropped from the Assets
/// panel. The UI Designer's inspector also picks from the project's images.
pub(super) fn image_control(ui: &mut egui::Ui, path: &str) -> Option<Edit> {
    let mut edit = None;
    let row = ui.horizontal(|ui| {
        if ui
            .small_button(icons::UPLOAD_SIMPLE)
            .on_hover_text("Upload an image from this computer (copied into assets/images)")
            .clicked()
        {
            edit = Some(Edit::PickTexture("Image"));
        }
        if let Some(value) = value_edit(ui, "Image", Value::String(path.to_owned())) {
            edit = Some(Edit::Property("Image", value));
        }
    });
    if let Some(held) = row
        .response
        .dnd_hover_payload::<crate::asset_browser::AssetPayload>()
        && matches!(
            held.0.kind,
            crate::asset_browser::AssetKind::Image | crate::asset_browser::AssetKind::Texture
        )
    {
        let palette = theme::palette(ui.ctx());
        ui.painter().rect_stroke(
            row.response.rect.expand(2.0),
            4,
            egui::Stroke::new(1.5_f32, palette.accent),
            egui::StrokeKind::Outside,
        );
        if let Some(payload) = row
            .response
            .dnd_release_payload::<crate::asset_browser::AssetPayload>()
        {
            edit = Some(Edit::Property(
                "Image",
                Value::String(payload.0.path.clone()),
            ));
        }
    }
    edit
}

/// A ColorCorrectionEffect's lookup table or palette file, with buttons to
/// pick one and to use none.
pub(super) fn grading_control(
    ui: &mut egui::Ui,
    name: &'static str,
    path: &str,
    assets: &Assets,
) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        let hover = if name == "LookupTable" {
            "Choose a lookup table: a .cube file, or a strip image of N squares of N × N \
             pixels side by side (such as 256 × 16)"
        } else {
            "Choose a palette: an image whose colors (up to 64) every pixel is snapped to"
        };
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text(hover)
            .clicked()
        {
            edit = Some(Edit::PickGradingFile(name));
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Use none")
            .clicked()
        {
            edit = Some(Edit::Property(name, Value::String(String::new())));
        }
        file_label(ui, assets, path);
    });
    edit
}

/// Lighting's sky image, with buttons to pick a file, to use the engine's
/// default sky and to go back to the sky colors. A sky dragged from the
/// Assets panel can be let go on it. All ask before the sky changes.
pub(super) fn sky_control(ui: &mut egui::Ui, path: &str, assets: &Assets) -> Option<Edit> {
    let mut edit = None;
    // One column: in the Properties grid a cell lays its pieces out left to
    // right, so without it the hint would go beside the file, squeezed into
    // whatever width was left, one letter to a line.
    let control = ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if ui
                .small_button(icons::FOLDER_OPEN)
                .on_hover_text(
                    "Choose a sky image from this computer: a 2:1 panorama, best as an HDR \
                     (.exr or .hdr), or a PNG or JPEG",
                )
                .clicked()
            {
                edit = Some(Edit::PickSkyImage);
            }
            if path != axcenity_project::DEFAULT_SKY_PATH
                && ui
                    .small_button(icons::SUN_HORIZON)
                    .on_hover_text(
                        "Use the default sky: a partly cloudy midday HDR (CC0, Poly Haven), \
                         with the sun moved to match it",
                    )
                    .clicked()
            {
                edit = Some(Edit::UseDefaultSky);
            }
            if path.is_empty() {
                ui.weak("Sky colors");
                return;
            }
            if ui
                .small_button(icons::X)
                .on_hover_text("Go back to the sky colors")
                .clicked()
            {
                edit = Some(Edit::Property("SkyImage", Value::String(String::new())));
            }
            file_label(ui, assets, path);
        });
        if !path.is_empty() {
            // A picture can't change with the time of day: say so, under
            // the file and as wide as the column, with the way to a sky
            // that does.
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "A sky picture keeps its colors: ClockTime moves the sun and dims \
                         it toward night.",
                    )
                    .small()
                    .weak(),
                )
                .wrap(),
            );
            if ui
                .small_button("Follow time of day")
                .on_hover_text(
                    "Go back to a drawn sky that follows ClockTime, with an Atmosphere so \
                     it turns orange at sunrise and sunset and dark at night",
                )
                .clicked()
            {
                edit = Some(Edit::UseTimeOfDaySky);
            }
        }
    });
    // The whole control, hint included, takes a sky dropped on it.
    let target = control.response;
    if let Some(held) = target.dnd_hover_payload::<crate::asset_browser::AssetPayload>()
        && held.0.kind == crate::asset_browser::AssetKind::Sky
    {
        let palette = theme::palette(ui.ctx());
        ui.painter().rect_stroke(
            target.rect.expand(2.0),
            4,
            egui::Stroke::new(1.5_f32, palette.accent),
            egui::StrokeKind::Outside,
        );
        if let Some(payload) = target.dnd_release_payload::<crate::asset_browser::AssetPayload>() {
            edit = Some(Edit::AskSkyImage(payload.0.path.clone()));
        }
    }
    edit
}

/// A property Animation's clip file.
pub(super) fn clip_control(ui: &mut egui::Ui, path: &str, assets: &Assets) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose a property clip (.anim.json with kind property) in the project")
            .clicked()
        {
            edit = Some(Edit::PickClip);
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::FILM_SLATE)
            .on_hover_text("Open it in the Animation tab")
            .clicked()
        {
            edit = Some(Edit::OpenClip);
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Play no clip")
            .clicked()
        {
            edit = Some(Edit::Property("Clip", Value::String(String::new())));
        }
        file_label(ui, assets, path);
    });
    edit
}

/// An Animator's controller file, with buttons to pick another and to clear it.
pub(super) fn controller_control(ui: &mut egui::Ui, path: &str, assets: &Assets) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose a state machine (.statemachine.json) in the project")
            .clicked()
        {
            edit = Some(Edit::PickController);
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::FLOW_ARROW)
            .on_hover_text("Open it in the Animation tab: its states, transitions and parameters")
            .clicked()
        {
            edit = Some(Edit::OpenController);
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Play no state machine")
            .clicked()
        {
            edit = Some(Edit::Property("StateMachine", Value::String(String::new())));
        }
        file_label(ui, assets, path);
    });
    edit
}

/// A NavigationAgent's behavior tree file, with buttons to pick another,
/// open it and clear it.
pub(super) fn behavior_tree_control(ui: &mut egui::Ui, path: &str) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose a behavior tree (.behaviortree.json) in the project")
            .clicked()
        {
            edit = Some(Edit::PickBehaviorTree);
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::BRAIN)
            .on_hover_text("Open it in the Behavior Tree tab")
            .clicked()
        {
            edit = Some(Edit::OpenBehaviorTree);
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Run no tree: scripts set where it walks")
            .clicked()
        {
            edit = Some(Edit::Property("BehaviorTree", Value::String(String::new())));
        }
        let name = axcenity_blocks::behavior_tree::behavior_tree_name(path);
        ui.add(egui::Label::new(name).truncate())
            .on_hover_text(path);
    });
    edit
}

/// A script's `SourceFile` (G2), with buttons to pick another and to open
/// it. Empty for a script from the `scripts` folder, which is its file.
pub(super) fn source_file_control(ui: &mut egui::Ui, path: &str) -> Option<Edit> {
    if path.is_empty() {
        ui.weak("Its own file in scripts/");
        return None;
    }
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text(
                "Choose the file this script runs. Copies of its object keep using theirs.",
            )
            .clicked()
        {
            edit = Some(Edit::PickSourceFile);
        }
        if ui
            .small_button(icons::ARROW_SQUARE_OUT)
            .on_hover_text("Open it in a tab")
            .clicked()
        {
            edit = Some(Edit::OpenSourceFile);
        }
        if ui
            .small_button(icons::PENCIL_SIMPLE)
            .on_hover_text(
                "Rename or move the file. Every script that uses it is changed to match.",
            )
            .clicked()
        {
            edit = Some(Edit::MoveSourceFile);
        }
        ui.add(egui::Label::new(path).truncate())
            .on_hover_text(path);
    });
    edit
}

/// A Sound's file, with buttons to pick another, and to hear it.
pub(super) fn sound_control(ui: &mut egui::Ui, path: &str, assets: &Assets) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose a sound file (WAV, OGG, MP3 or FLAC)")
            .clicked()
        {
            edit = Some(Edit::PickAudio);
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::PLAY)
            .on_hover_text("Listen to it here, without pressing Play")
            .clicked()
        {
            edit = Some(Edit::PreviewSound(true));
        }
        if ui
            .small_button(icons::STOP)
            .on_hover_text("Stop listening")
            .clicked()
        {
            edit = Some(Edit::PreviewSound(false));
        }
        file_label(ui, assets, path);
    });
    edit
}
