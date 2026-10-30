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
            None,
            theme::ThemeChoice::Dark,
        ),
        (
            "start-projects-light",
            Page::Projects,
            None,
            theme::ThemeChoice::Light,
        ),
        ("start-learn", Page::Learn, None, theme::ThemeChoice::Dark),
        (
            "start-reference",
            Page::Reference,
            None,
            theme::ThemeChoice::Dark,
        ),
        (
            "start-new-project",
            Page::Projects,
            Some(Template::Starter),
            theme::ThemeChoice::Dark,
        ),
        // A genre template picked: its picture, and the rest beside it.
        (
            "start-new-project-arena",
            Page::Projects,
            Some(Template::Arena),
            theme::ThemeChoice::Light,
        ),
    ] {
        let mut start = StartScreen {
            page,
            ..Default::default()
        };
        if let Some(template) = dialog {
            start.new_project(template);
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

#[test]
fn searching_the_reference_opens_classes_properties_and_functions() {
    let entries = axcenity_script::api_reference();
    let first = |query: &str| {
        let results = reference::search_entries(&entries, query);
        entries[results[0]].qualified_name()
    };
    assert_eq!(first("Part"), "Part");
    assert_eq!(first("part.size"), "Part.Size");
    assert_eq!(first("Instance.new"), "Instance.new");
    assert_eq!(first("instance:findfirstchild"), "Instance.FindFirstChild");
    // A member name finds members of every class that has it.
    let sizes = reference::search_entries(&entries, "Size");
    assert!(sizes.len() > 1);
    assert!(sizes.iter().all(|&i| entries[i].name.contains("Size")));
    assert!(reference::search_entries(&entries, "no such thing").is_empty());
}

#[test]
fn the_reference_draws_results_and_an_open_entry() {
    let entries = axcenity_script::api_reference();
    let part = entries
        .iter()
        .position(|entry| entry.owner.is_empty() && entry.name == "Part")
        .unwrap();
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    for (name, query, open) in [
        ("start-reference-search", "Size", None),
        ("start-reference-entry", "Part", Some(part)),
    ] {
        let mut start = StartScreen {
            page: Page::Reference,
            ..Default::default()
        };
        start.reference.query = query.to_owned();
        start.reference.open = open;
        let mut projects = Vec::new();
        let mut draw = |ctx: &egui::Context| {
            theme::apply(ctx, &theme::Appearance::default());
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
        assert_eq!(start.reference.open, open, "drawing alone opens nothing");
    }
}

#[test]
fn every_tutorial_names_a_sample_that_loads() {
    for tutorial in learn::TUTORIALS {
        let folder = learn::find_sample(tutorial.sample)
            .unwrap_or_else(|| panic!("examples/{} wasn't found", tutorial.sample));
        let project = axcenity_project::load(&folder).unwrap();
        assert_eq!(
            project.manifest.name, tutorial.sample_name,
            "{}",
            tutorial.title
        );
        assert!(!tutorial.steps.is_empty(), "{}", tutorial.title);
    }
    // The four the roadmap asks for come first.
    let titles: Vec<&str> = learn::TUTORIALS[..4].iter().map(|t| t.title).collect();
    assert_eq!(
        titles,
        [
            "Place and move a part",
            "Run a script",
            "Press Play and Stop",
            "Open a sample"
        ]
    );
}

#[test]
fn every_look_up_opens_its_api_reference_entry() {
    for tutorial in learn::TUTORIALS {
        for &name in tutorial.look_up {
            let mut start = StartScreen::default();
            start.reference.look_up(name);
            let open = start
                .reference
                .open
                .map(|i| start.reference.entries[i].qualified_name());
            assert_eq!(open.as_deref(), Some(name), "{}", tutorial.title);
        }
    }
}

#[test]
fn the_learn_page_draws_every_tutorial() {
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    for (index, tutorial) in learn::TUTORIALS.iter().enumerate() {
        for theme in [theme::ThemeChoice::Dark, theme::ThemeChoice::Light] {
            let mut start = StartScreen {
                page: Page::Learn,
                ..Default::default()
            };
            start.learn.open = Some(index);
            let appearance = theme::Appearance {
                theme,
                ..Default::default()
            };
            let mut projects = Vec::new();
            let mut draw = |ctx: &egui::Context| {
                theme::apply(ctx, &appearance);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
                    .show(ctx, |ui| {
                        let brand = Brand::load(ui.ctx());
                        assert!(start.show(ui, &mut projects, &brand).is_none());
                    });
            };
            let light = if theme == theme::ThemeChoice::Light {
                "-light"
            } else {
                ""
            };
            match &snapshots {
                Some(folder) => crate::snapshot::snapshot(
                    &folder.join(format!("start-learn-{}{light}.png", tutorial.sample)),
                    vec2(1400.0, 2400.0),
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
            assert_eq!(start.learn.open, Some(index), "drawing alone goes nowhere");
        }
    }
}

#[test]
fn the_learn_window_shows_tutorials_and_the_reference_over_a_project() {
    let mut start = StartScreen::default();
    start.learn.open = Some(0);
    let sample = learn::find_sample(learn::TUTORIALS[0].sample).unwrap();
    let ctx = theme::test_context();
    for page in [Page::Learn, Page::Reference] {
        start.page = page;
        for _ in 0..3 {
            let _ = ctx.run(Default::default(), |ctx| {
                theme::apply(ctx, &theme::Appearance::default());
                let mut open = true;
                let picked = start.learn_window(ctx, &mut open, Some(&sample), false);
                assert!(picked.is_none() && open);
            });
        }
        assert_eq!(start.page, page);
    }
}

#[test]
fn the_global_assets_page_lists_the_library_with_a_models_files_as_one_card() {
    let library =
        std::env::temp_dir().join(format!("axcenity-start-global-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&library);
    for file in [
        "assets/models/Hero/Hero.fbx",
        "assets/models/Hero/textures/Hero_Diffuse.png",
        "assets/animations/Hero/Walk.anim.json",
        "assets/audio/ding.wav",
    ] {
        let path = library.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }
    let mut start = StartScreen {
        page: Page::Global,
        ..Default::default()
    };
    start.set_library(Some(library.clone()));
    let names = |start: &mut StartScreen, search: &str| -> Vec<String> {
        start
            .global
            .cards(search)
            .into_iter()
            .map(|card| card.name)
            .collect()
    };
    assert_eq!(names(&mut start, ""), ["Hero", "ding"]);
    assert_eq!(names(&mut start, "walk"), ["Walk"]);

    let mut projects = Vec::new();
    let mut draw = |ctx: &egui::Context| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
            .show(ctx, |ui| {
                let brand = Brand::load(ui.ctx());
                assert!(start.show(ui, &mut projects, &brand).is_none());
            });
    };
    match std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        Some(folder) => crate::snapshot::snapshot(
            &folder.join("start-global-assets.png"),
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
    std::fs::remove_dir_all(library).unwrap();
}

#[test]
fn the_learn_page_says_build_game() {
    let notes: Vec<&str> = learn::TOPICS.iter().map(|&(_, _, note)| note).collect();
    assert!(
        notes.iter().any(|note| note.contains("File > Build Game")),
        "{notes:?}"
    );
    let tutorials = learn::TUTORIALS
        .iter()
        .flat_map(|tutorial| tutorial.steps.iter().map(|step| step.text));
    for text in notes.iter().copied().chain(tutorials) {
        assert!(!text.contains("Export Game"), "{text}");
    }
}
