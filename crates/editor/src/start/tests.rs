use super::*;

fn link(name: &str) -> ProjectLink {
    ProjectLink {
        name: name.to_string(),
        description: String::new(),
        folder: PathBuf::from(name),
    }
}

#[test]
fn recent_projects_put_the_newest_first_without_repeats() {
    let mut recent = Vec::new();
    for name in ["a", "b", "a"] {
        remember(&mut recent, link(name));
    }
    assert_eq!(recent, [link("a"), link("b")]);

    for n in 0..MAX_TRACKED + 3 {
        remember(&mut recent, link(&n.to_string()));
    }
    assert_eq!(recent.len(), MAX_TRACKED);
    assert_eq!(recent[0], link(&(MAX_TRACKED + 2).to_string()));
}

#[test]
fn the_start_screen_draws_each_page_and_the_new_project_dialog() {
    let mut projects: Vec<ProjectLink> = ["Space Race", "Obby", "Castle Builder"]
        .into_iter()
        .map(|name| ProjectLink {
            name: name.to_owned(),
            description: format!("{name}, a test project"),
            folder: PathBuf::from(format!("not-a-folder/{name}")),
        })
        .collect();
    // One has a thumbnail: a bright picture, so its corners show.
    let pictured = std::env::temp_dir().join(format!("axcenity-thumb-{}", std::process::id()));
    std::fs::create_dir_all(&pictured).unwrap();
    let mut rgb = Vec::new();
    for y in 0..180u32 {
        for x in 0..320u32 {
            rgb.extend([(x * 255 / 320) as u8, (y * 255 / 180) as u8, 230]);
        }
    }
    let mut png_bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut png_bytes, 320, 180);
    encoder.set_color(png::ColorType::Rgb);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rgb)
        .unwrap();
    std::fs::write(pictured.join(axcenity_project::THUMBNAIL_FILE), png_bytes).unwrap();
    projects[0].folder = pictured.clone();
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    for (name, page, dialog, theme) in [
        (
            "start-projects",
            Page::Projects,
            false,
            theme::ThemeChoice::Dark,
        ),
        (
            "start-projects-light",
            Page::Projects,
            false,
            theme::ThemeChoice::Light,
        ),
        ("start-learn", Page::Learn, false, theme::ThemeChoice::Dark),
        (
            "start-new-project",
            Page::Projects,
            true,
            theme::ThemeChoice::Dark,
        ),
    ] {
        let mut start = StartScreen {
            page,
            ..Default::default()
        };
        if dialog {
            start.new_project(Template::Starter);
        }
        let appearance = theme::Appearance {
            theme,
            ..Default::default()
        };
        let mut draw = |ctx: &egui::Context| {
            theme::apply(ctx, &appearance);
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
                .show(ctx, |ui| {
                    let brand = Brand::load(ui.ctx());
                    assert!(start.show(ui, &mut projects, &brand).is_none());
                });
        };
        match &snapshots {
            Some(folder) => crate::snapshot::snapshot(
                &folder.join(format!("{name}.png")),
                vec2(1400.0, 900.0),
                vec![Vec::new(); 3],
                &mut draw,
            ),
            None => {
                let ctx = theme::test_context();
                for _ in 0..3 {
                    let _ = ctx.run(Default::default(), &mut draw);
                }
            }
        }
    }
    assert_eq!(projects.len(), 3, "drawing alone changes nothing");
    std::fs::remove_dir_all(pictured).unwrap();
}

#[test]
fn edit_times_read_like_people_say_them() {
    let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10_000_000);
    let before = |seconds| now - std::time::Duration::from_secs(seconds);
    assert_eq!(ago(before(5), now), "just now");
    assert_eq!(ago(before(60), now), "1 minute ago");
    assert_eq!(ago(before(7_200), now), "2 hours ago");
    assert_eq!(ago(before(90_000), now), "yesterday");
    assert_eq!(ago(before(5 * 86_400), now), "5 days ago");
    // A clock set back doesn't panic.
    assert_eq!(
        ago(now + std::time::Duration::from_secs(9), now),
        "just now"
    );
}

#[test]
fn only_project_folders_can_be_deleted() {
    let folder = std::env::temp_dir().join(format!("axcenity-delete-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    assert!(delete_project(&folder).is_err());
    assert!(folder.exists());
    std::fs::write(folder.join(axcenity_project::MANIFEST_FILE), "{}").unwrap();
    delete_project(&folder).unwrap();
    assert!(!folder.exists());
}
