use super::*;

/// The Learn page: where to start with each part of the engine.
pub(super) fn learn_page(ui: &mut egui::Ui) {
    let palette = theme::palette(ui.ctx());
    ui.label(
        RichText::new("Learn")
            .family(theme::heading_family())
            .size(type_scale::DISPLAY)
            .color(palette.text_strong),
    );
    ui.label(
        RichText::new("The basics of each part of the editor. Open or make a project to try them.")
            .color(palette.text_secondary),
    );
    ui.add_space(24.0);
    let topics = [
        (
            icons::CUBE,
            "Build",
            "Insert parts from the toolbar, then move, scale and rotate them with the \
             handles (Ctrl+2, 3, 4). Drag a part by its body to drop it onto others.",
        ),
        (
            icons::PUZZLE_PIECE,
            "Blocks",
            "Toolbar > Server Blocks makes a block script. Drag blocks from the palette, \
             snap them under an event, and press View as Lua to see what they do.",
        ),
        (
            icons::CODE,
            "Lua scripts",
            "Scripts run on the server, LocalScripts on each player's side. File > Set Up \
             VS Code Autocomplete gives you the whole API as you type.",
        ),
        (
            icons::LAYOUT,
            "UI and CSS",
            "Insert a Frame, Text or Button from the UI group. Drag its handles to place \
             it, or style it with CSS in the Style section of Properties.",
        ),
        (
            icons::SUN,
            "Lighting",
            "Select Lighting in the Explorer for shadows (with softness), the sky and \
             bloom. Turn off CastShadow on a part to stop it casting a shadow.",
        ),
        (
            icons::SPEAKER_HIGH,
            "Sound",
            "Insert a Sound into a part, pick a file in Properties and press ▶ to hear it. \
             Turn on Playing to start it with the game.",
        ),
        (
            icons::PLAY,
            "Play and export",
            "F5 plays the game in the viewport and Shift+F5 stops it. File > Export Game \
             makes a game anyone can run without the editor.",
        ),
    ];
    let width = ui.available_width();
    let columns = ((width + CARD_SPACING) / (260.0 + CARD_SPACING))
        .floor()
        .max(1.0);
    let card_width = (width - CARD_SPACING * (columns - 1.0)) / columns;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
        for (icon, title, text) in topics {
            let (rect, response) = ui.allocate_exact_size(vec2(card_width, 128.0), Sense::hover());
            let hover = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
            card_frame(ui, rect, &response, hover, &palette);
            ui.painter().text(
                rect.min + vec2(16.0, 24.0),
                egui::Align2::LEFT_CENTER,
                icon,
                FontId::proportional(16.0),
                palette.accent,
            );
            ui.painter().text(
                rect.min + vec2(40.0, 24.0),
                egui::Align2::LEFT_CENTER,
                title,
                FontId::new(type_scale::BODY, theme::medium_family()),
                palette.text_strong,
            );
            let galley = ui.painter().layout(
                text.to_owned(),
                FontId::proportional(type_scale::SMALL + 0.5),
                palette.text_secondary,
                rect.width() - 32.0,
            );
            ui.painter().with_clip_rect(rect.shrink(1.0)).galley(
                rect.min + vec2(16.0, 44.0),
                galley,
                palette.text_secondary,
            );
        }
    });
}

/// Deletes a project folder, only if it really is one.
pub(super) fn delete_project(folder: &Path) -> Result<(), String> {
    if !folder.join(axcenity_project::MANIFEST_FILE).is_file() {
        return Err("That folder isn't a project any more, so it was left alone.".into());
    }
    std::fs::remove_dir_all(folder).map_err(|err| format!("Could not delete it: {err}"))
}

/// Shows `folder` in the system's file manager.
pub(super) fn reveal(folder: &Path) -> std::io::Result<()> {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(folder).spawn()?;
    Ok(())
}

/// "3 days ago", for when a project was last saved.
pub(super) fn ago(then: SystemTime, now: SystemTime) -> String {
    let seconds = now
        .duration_since(then)
        .map_or(0, |elapsed| elapsed.as_secs());
    let plural = |count: u64, unit: &str| {
        if count == 1 {
            format!("1 {unit} ago")
        } else {
            format!("{count} {unit}s ago")
        }
    };
    match seconds {
        0..60 => "just now".to_owned(),
        60..3600 => plural(seconds / 60, "minute"),
        3600..86_400 => plural(seconds / 3600, "hour"),
        86_400..172_800 => "yesterday".to_owned(),
        172_800..2_592_000 => plural(seconds / 86_400, "day"),
        2_592_000..31_536_000 => plural(seconds / 2_592_000, "month"),
        _ => plural(seconds / 31_536_000, "year"),
    }
}
