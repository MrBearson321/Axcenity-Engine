use super::*;

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
