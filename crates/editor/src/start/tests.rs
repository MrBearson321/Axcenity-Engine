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
    let folded = StartNav {
        folded: true,
        ..Default::default()
    };
    let top_bar = StartNav {
        layout: NavLayout::TopBar,
        folded: false,
    };
    for (name, page, dialog, theme, nav) in [
        (
            "start-projects",
            Page::Projects,
            None,
            theme::ThemeChoice::Dark,
            StartNav::default(),
        ),
        (
            "start-projects-light",
            Page::Projects,
            None,
            theme::ThemeChoice::Light,
            StartNav::default(),
        ),
        (
            "start-learn",
            Page::Learn,
            None,
            theme::ThemeChoice::Dark,
            StartNav::default(),
        ),
        (
            "start-reference",
            Page::Reference,
            None,
            theme::ThemeChoice::Dark,
            StartNav::default(),
        ),
        (
            "start-new-project",
            Page::Projects,
            Some(Template::Empty),
            theme::ThemeChoice::Dark,
            StartNav::default(),
        ),
        // A genre template picked: its picture, and the rest beside it.
        (
            "start-new-project-arena",
            Page::Projects,
            Some(Template::Arena),
            theme::ThemeChoice::Light,
            StartNav::default(),
        ),
        (
            "start-projects-folded",
            Page::Projects,
            None,
            theme::ThemeChoice::Dark,
            folded,
        ),
        (
            "start-projects-top-bar",
            Page::Projects,
            None,
            theme::ThemeChoice::Dark,
            top_bar,
        ),
        (
            "start-learn-top-bar-light",
            Page::Learn,
            None,
            theme::ThemeChoice::Light,
            top_bar,
        ),
    ] {
        let mut start = StartScreen {
            page,
            nav,
            ..Default::default()
        };
        if let Some(template) = dialog {
            start.new_project(template);
        }
        let appearance = theme::Appearance {
            theme,
            ..Default::default()
        };
        let mut draw = |ui: &mut egui::Ui| {
            let ctx = &ui.ctx().clone();
            theme::apply(ctx, &appearance);
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
                .show(ui, |ui| {
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
                    let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
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
    // Part.Touched has a usage example and pages that explain it.
    let touched = entries
        .iter()
        .position(|entry| entry.qualified_name() == "Part.Touched")
        .unwrap();
    assert!(axcenity_script::api_example("Part.Touched").is_some());
    assert!(!docs::markdown::pages_mentioning("Part.Touched").is_empty());
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    for (name, query, open) in [
        ("start-reference-search", "Size", None),
        ("start-reference-entry", "Part", Some(part)),
        ("start-reference-example", "Touched", Some(touched)),
    ] {
        let mut start = StartScreen {
            page: Page::Reference,
            ..Default::default()
        };
        start.reference.query = query.to_owned();
        start.reference.open = open;
        let mut projects = Vec::new();
        let mut draw = |ui: &mut egui::Ui| {
            let ctx = &ui.ctx().clone();
            theme::apply(ctx, &theme::Appearance::default());
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
                .show(ui, |ui| {
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
                    let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
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
            let mut draw = |ui: &mut egui::Ui| {
                let ctx = &ui.ctx().clone();
                theme::apply(ctx, &appearance);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
                    .show(ui, |ui| {
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
                        let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
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
    for page in [Page::Learn, Page::Docs, Page::Reference] {
        start.page = page;
        for _ in 0..3 {
            let _ = crate::theme::run(&ctx, Default::default(), |ui| {
                let ctx = &ui.ctx().clone();
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
    let mut draw = |ui: &mut egui::Ui| {
        let ctx = &ui.ctx().clone();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
            .show(ui, |ui| {
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
                let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
            }
        }
    }
    std::fs::remove_dir_all(library).unwrap();
}

#[test]
fn the_learn_page_says_build_game() {
    let library = docs::library();
    let chapters: Vec<&docs::library::Page> = learn::chapters(library)
        .into_iter()
        .map(|(_, page)| &library.pages[page])
        .collect();
    assert!(
        chapters.iter().any(|page| {
            page.blocks
                .iter()
                .any(|block| format!("{block:?}").contains("File > Build Game"))
        }),
        "a chapter of the learning path says how to build a game"
    );
    let tutorials = learn::TUTORIALS
        .iter()
        .flat_map(|tutorial| tutorial.steps.iter().map(|step| step.text));
    for text in tutorials {
        assert!(!text.contains("Export Game"), "{text}");
    }
    for page in &library.pages {
        for block in &page.blocks {
            assert!(
                !format!("{block:?}").contains("Export Game"),
                "{}",
                page.path
            );
        }
    }
}

/// Every button drawn and its bounds, with AccessKit on: label (without
/// icon characters), left and right edges, and top.
fn drawn_buttons(output: &egui::FullOutput) -> Vec<(String, f32, f32, f32)> {
    let update = output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("AccessKit is on");
    update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == egui::accesskit::Role::Button)
        .filter_map(|(_, node)| {
            let label: String = node
                .label()?
                .chars()
                .filter(|c| !('\u{e000}'..='\u{f8ff}').contains(c))
                .collect();
            let bounds = node.bounds()?;
            Some((
                label.trim().to_owned(),
                bounds.x0 as f32,
                bounds.x1 as f32,
                bounds.y0 as f32,
            ))
        })
        .collect()
}

#[test]
fn recent_project_cards_stay_inside_the_page_at_every_window_width() {
    let names: Vec<String> = (1..=9).map(|n| format!("Project {n}")).collect();
    for width in [900.0, 1280.0, 1785.0, 2200.0, 3000.0] {
        let mut projects: Vec<ProjectLink> = names
            .iter()
            .map(|name| ProjectLink {
                name: name.clone(),
                description: String::new(),
                folder: PathBuf::from(format!("not-a-folder/{name}")),
            })
            .collect();
        let mut start = StartScreen::default();
        let ctx = theme::test_context();
        ctx.enable_accesskit();
        let mut output = None;
        for frame in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 4000.0))),
                time: Some(frame as f64 * 10.0),
                ..Default::default()
            };
            output = Some(crate::theme::run(&ctx, input, |ui| {
                let ctx = &ui.ctx().clone();
                theme::apply(ctx, &theme::Appearance::default());
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
                    .show(ui, |ui| {
                        let brand = Brand::load(ui.ctx());
                        assert!(start.show(ui, &mut projects, &brand).is_none());
                    });
            }));
        }
        let buttons = drawn_buttons(&output.unwrap());
        let edge = |label: &str| {
            buttons
                .iter()
                .find(|(found, ..)| found == label)
                .unwrap_or_else(|| panic!("`{label}` is drawn at {width}"))
                .clone()
        };
        // The header's New Project button ends where the page does.
        let (_, _, page_right, _) = edge("New Project");
        let cards: Vec<_> = buttons
            .iter()
            .filter(|(label, ..)| label.starts_with("Project "))
            .cloned()
            .collect();
        // Recent has the four last opened, All projects the first eight; the
        // ninth waits behind Show 1 more.
        assert_eq!(
            cards.len(),
            RECENT_PROJECTS + ALL_PROJECTS_SHOWN,
            "at {width}"
        );
        assert!(cards.iter().all(|(name, ..)| name != "Project 9"));
        edge("Show 1 more");
        let page_left = cards.iter().map(|card| card.1).fold(f32::MAX, f32::min);
        for (name, left, right, _) in &cards {
            assert!(
                *left >= page_left - 0.5 && *right <= page_right + 0.5,
                "`{name}` spans {left}..{right}, outside the page {page_left}..{page_right} \
                 in a window {width} wide"
            );
        }
        // More cards than fit in a row wrap onto the next ones, in columns,
        // each row starting at the left.
        let (columns, _) = grid_columns(page_right - page_left);
        let mut rows = std::collections::BTreeMap::<i32, f32>::new();
        for (_, left, _, top) in &cards {
            let row = rows.entry(top.round() as i32).or_insert(f32::MAX);
            *row = row.min(*left);
        }
        assert_eq!(
            rows.len(),
            RECENT_PROJECTS.div_ceil(columns) + ALL_PROJECTS_SHOWN.div_ceil(columns),
            "rows at {width}"
        );
        for left in rows.values() {
            assert!(
                (left - page_left).abs() < 0.5,
                "a new row starts at the left"
            );
        }
        // The screenshot's width: look at it with AXCENITY_SNAPSHOTS.
        if width == 1785.0
            && let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from)
        {
            let mut start = StartScreen::default();
            crate::snapshot::snapshot(
                &folder.join("start-projects-wide.png"),
                vec2(width, 1100.0),
                vec![Vec::new(); 3],
                |ui| {
                    let ctx = &ui.ctx().clone();
                    theme::apply(ctx, &theme::Appearance::default());
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
                        .show(ui, |ui| {
                            let brand = Brand::load(ui.ctx());
                            assert!(start.show(ui, &mut projects, &brand).is_none());
                        });
                },
            );
        }
    }
}

#[test]
fn the_version_label_names_the_channel() {
    let label = version_label();
    let version = axcenity_release::Version::current();
    assert_eq!(label, format!("{version} · {}", version.channel()));
}

#[test]
fn the_about_box_build_line_names_the_commit_and_toolchain() {
    let mut build = axcenity_release::BuildInfo::current();
    build.commit = "0123456789abcdef0123456789abcdef01234567".to_owned();
    build.rustc = "rustc 1.97.0 (2d8144b78 2026-07-07)".to_owned();
    build.target = "x86_64-pc-windows-msvc".to_owned();
    build.profile = "dist".to_owned();
    build.kind = axcenity_release::BuildKind::Local;
    assert_eq!(
        build_line(&build),
        "Local build · commit 012345678 · rustc 1.97.0 · x86_64-pc-windows-msvc (dist)"
    );
    build.kind = axcenity_release::BuildKind::Official;
    build.tag = Some("v0.2.0-alpha.1".to_owned());
    assert!(build_line(&build).starts_with("Official build v0.2.0-alpha.1 · commit"));
}

#[test]
fn copy_diagnostics_names_the_build_system_gpu_and_known_issues() {
    let mut build = axcenity_release::BuildInfo::current();
    build.version = axcenity_release::Version::parse("0.1.0-alpha.3").unwrap();
    build.commit = "0123456789abcdef0123456789abcdef01234567".to_owned();
    let text = diagnostics(&build, Some("Test GPU (Vulkan)"));
    assert!(text.starts_with("Axcenity Editor "), "{text}");
    assert!(text.contains("commit: 0123456789abcdef"), "{text}");
    assert!(text.contains("channel: Development"), "{text}");
    assert!(
        text.contains(&format!("os: {} ", std::env::consts::OS)),
        "{text}"
    );
    assert!(text.contains("gpu: Test GPU (Vulkan)"), "{text}");
    assert!(text.contains("install: built from source"), "{text}");
    // 0.1.0-alpha.3 was checked and found affected by AX-0001.
    assert!(text.contains("known issues: AX-0001"), "{text}");
    assert!(diagnostics(&build, None).contains("gpu: not started yet"));
}

/// Every label and button drawn, with AccessKit on: its text (without icon
/// characters; a label's starts "label: ") and its bounds as left, right,
/// top and bottom.
fn drawn_text(output: &egui::FullOutput) -> Vec<(String, f32, f32, f32, f32)> {
    let update = output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("AccessKit is on");
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let (kind, text) = match node.role() {
                egui::accesskit::Role::Label => ("label: ", node.value()?),
                egui::accesskit::Role::Button => ("", node.label()?),
                _ => return None,
            };
            let text: String = text
                .chars()
                .filter(|c| !('\u{e000}'..='\u{f8ff}').contains(c))
                .collect();
            let text = format!("{kind}{}", text.trim());
            let bounds = node.bounds()?;
            Some((
                text.trim().to_owned(),
                bounds.x0 as f32,
                bounds.x1 as f32,
                bounds.y0 as f32,
                bounds.y1 as f32,
            ))
        })
        .collect()
}

/// The start screen drawn `width` wide with `nav`, after it settles.
fn draw_with_nav(nav: StartNav, width: f32) -> Vec<(String, f32, f32, f32, f32)> {
    let mut projects = vec![link("Space Race")];
    let mut start = StartScreen::default();
    start.set_nav(nav);
    let ctx = theme::test_context();
    ctx.enable_accesskit();
    let mut output = None;
    for frame in 0..4 {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 800.0))),
            time: Some(frame as f64 * 10.0),
            ..Default::default()
        };
        output = Some(crate::theme::run(&ctx, input, |ui| {
            let ctx = &ui.ctx().clone();
            theme::apply(ctx, &theme::Appearance::default());
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
                .show(ui, |ui| {
                    let brand = Brand::load(ui.ctx());
                    assert!(start.show(ui, &mut projects, &brand).is_none());
                });
        }));
    }
    drawn_text(&output.unwrap())
}

fn find<'a>(
    drawn: &'a [(String, f32, f32, f32, f32)],
    text: &str,
) -> &'a (String, f32, f32, f32, f32) {
    drawn
        .iter()
        .find(|(found, ..)| found.contains(text))
        .unwrap_or_else(|| panic!("`{text}` is drawn: {drawn:?}"))
}

/// The navigation's button for `page` (not a heading of the same name),
/// the one nearest the top left.
fn nav_button<'a>(
    drawn: &'a [(String, f32, f32, f32, f32)],
    page: &str,
) -> &'a (String, f32, f32, f32, f32) {
    drawn
        .iter()
        .filter(|(text, ..)| text == page)
        .min_by(|a, b| (a.1 + a.3).total_cmp(&(b.1 + b.3)))
        .unwrap_or_else(|| panic!("`{page}` is drawn"))
}

#[test]
fn the_sidebar_version_stays_inside_the_sidebar() {
    for width in [900.0, 1400.0] {
        let drawn = draw_with_nav(StartNav::default(), width);
        for text in ["Axcenity Engine", "Development", "Apache-2.0"] {
            let (_, left, right, top, bottom) = find(&drawn, text);
            assert!(
                *left >= 0.0 && *right <= nav::SIDEBAR_WIDTH,
                "`{text}` spans {left}..{right}, past the sidebar's {} edge",
                nav::SIDEBAR_WIDTH
            );
            // Below Settings, so the two never overlap.
            let (_, _, _, _, settings_bottom) = find(&drawn, "Settings");
            assert!(top >= settings_bottom, "`{text}` overlaps Settings");
            assert!(*bottom <= 800.0, "`{text}` is cut off at the bottom");
        }
        // The page starts after the sidebar.
        let (_, new_left, ..) = find(&drawn, "New Project");
        assert!(*new_left > nav::SIDEBAR_WIDTH);
    }
}

#[test]
fn a_folded_sidebar_keeps_every_page_as_an_icon_and_gives_the_page_its_room() {
    let folded = StartNav {
        folded: true,
        ..Default::default()
    };
    let drawn = draw_with_nav(folded, 1280.0);
    for page in [
        "Projects",
        "Global Assets",
        "Learn",
        "Documentation",
        "API Reference",
        "Settings",
    ] {
        let (_, left, right, ..) = nav_button(&drawn, page);
        assert!(
            *left >= 0.0 && *right <= nav::FOLDED_WIDTH,
            "`{page}` spans {left}..{right} in a folded sidebar"
        );
    }
    // The version shows in the logo's tooltip, not as text that won't fit.
    assert!(drawn.iter().all(|(text, ..)| !text.contains("Apache")));
    find(&drawn, "Open the sidebar");
    // The page moves left to fill the room: wider than beside an open sidebar.
    let (_, folded_left, ..) = find(&drawn, "Space Race");
    let open = draw_with_nav(StartNav::default(), 1280.0);
    let (_, open_left, ..) = find(&open, "Space Race");
    assert!(folded_left < open_left, "{folded_left} vs {open_left}");
}

#[test]
fn the_top_bar_lists_the_same_pages_across_the_top() {
    let top = StartNav {
        layout: NavLayout::TopBar,
        folded: false,
    };
    for width in [800.0, 900.0, 1400.0] {
        let drawn = draw_with_nav(top, width);
        let mut lefts = Vec::new();
        for page in [
            "Projects",
            "Global Assets",
            "Learn",
            "Documentation",
            "API Reference",
            "Settings",
        ] {
            let (_, left, right, _, bottom) = nav_button(&drawn, page);
            assert!(*bottom <= nav::TOP_BAR_HEIGHT, "`{page}` is in the top bar");
            assert!(*right <= width, "`{page}` fits the window at {width}");
            lefts.push(*left);
        }
        assert!(lefts.is_sorted(), "left to right, then Settings: {lefts:?}");
        find(&drawn, "Show the pages in a sidebar");
        // The page uses the whole width, below the bar.
        let (_, card_left, _, card_top, _) = find(&drawn, "Space Race");
        assert!(*card_left < nav::SIDEBAR_WIDTH && *card_top > nav::TOP_BAR_HEIGHT);
    }
}

#[test]
fn the_layout_buttons_switch_between_sidebar_and_top_bar() {
    let mut projects = Vec::new();
    let mut start = StartScreen::default();
    let ctx = theme::test_context();
    ctx.enable_accesskit();
    // Each frame a while after the last, so folding has finished sliding.
    let mut clock = 0.0;
    let mut frame = |start: &mut StartScreen, events: Vec<egui::Event>| {
        clock += 10.0;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            time: Some(clock),
            events,
            ..Default::default()
        };
        crate::theme::run(&ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let brand = Brand::load(ui.ctx());
                let _ = start.show(ui, &mut projects, &brand);
            });
        })
    };
    let click = |at: egui::Pos2| {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        vec![egui::Event::PointerMoved(at), button(true), button(false)]
    };
    let center = |drawn: &[(String, f32, f32, f32, f32)], text: &str| {
        let (_, left, right, top, bottom) = find(drawn, text);
        pos2((left + right) / 2.0, (top + bottom) / 2.0)
    };
    frame(&mut start, Vec::new());
    let drawn = drawn_text(&frame(&mut start, Vec::new()));
    let fold = center(&drawn, "Fold the sidebar to its icons");
    frame(&mut start, click(fold));
    assert!(start.nav().folded);
    frame(&mut start, Vec::new());
    let drawn = drawn_text(&frame(&mut start, Vec::new()));
    let unfold = center(&drawn, "Open the sidebar");
    frame(&mut start, click(unfold));
    assert!(!start.nav().folded);
    frame(&mut start, Vec::new());
    let drawn = drawn_text(&frame(&mut start, Vec::new()));
    let top_bar = center(&drawn, "Show the pages in a top bar");
    frame(&mut start, click(top_bar));
    assert_eq!(start.nav().layout, NavLayout::TopBar);
    frame(&mut start, Vec::new());
    let drawn = drawn_text(&frame(&mut start, Vec::new()));
    let sidebar = center(&drawn, "Show the pages in a sidebar");
    frame(&mut start, click(sidebar));
    assert_eq!(start.nav().layout, NavLayout::Sidebar);
}

// Moved from widgets.rs when it went to the editor kit: it draws the
// start screen's brand mark.
#[test]
fn the_logo_entrances_and_the_spotlight_draw() {
    let pointer = egui::pos2(340.0, 200.0);
    let mut draw = |ui: &mut egui::Ui| {
        let ctx = &ui.ctx().clone();
        let brand = Brand::load(ctx);
        egui::CentralPanel::default().show(ui, |ui| {
            let palette = crate::theme::palette(ui.ctx());
            let (top, ink) = (ui.max_rect().top() + 20.0, palette.text_strong);
            let at = |index: usize, y: f32| {
                Rect::from_min_size(egui::pos2(20.0 + index as f32 * 100.0, y), vec2(80.0, 80.0))
            };
            for (index, t) in [0.25, 0.5, 0.75, 1.0].into_iter().enumerate() {
                crate::widgets::paint_mark_cut(ui.painter(), at(index, top), brand.mark(), ink, t);
            }
            for (index, position) in [0.3, 0.5, 0.7, 0.0].into_iter().enumerate() {
                let colors = (palette.text_muted, ink);
                crate::widgets::paint_mark_sweep(
                    ui.painter(),
                    at(index, top + 100.0),
                    brand.mark(),
                    colors,
                    position,
                );
            }
            crate::widgets::spotlight(
                ui,
                Rect::from_center_size(pointer, vec2(220.0, 120.0)),
                egui::Id::new("card"),
                1.0,
            );
            ui.horizontal(|ui| {
                let icons = crate::theme::icons::SQUARES_FOUR;
                let _ = crate::widgets::nav_rail(ui, icons, "Projects", Some(3), true, 1.0);
                let gear = crate::theme::icons::GEAR_SIX;
                let _ = crate::widgets::nav_rail(ui, gear, "Settings", None, false, 0.0);
            });
        });
    };
    let moved = vec![egui::Event::PointerMoved(pointer)];
    match std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from) {
        Some(folder) => crate::snapshot::snapshot(
            &folder.join("logo-entrances.png"),
            vec2(460.0, 280.0),
            vec![moved, Vec::new()],
            &mut draw,
        ),
        None => {
            let ctx = crate::theme::test_context();
            let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
        }
    }
}

/// Where every button sits on each frame (60 a second) while the sidebar
/// folds (`fold`) or opens, `width` wide, from a settled start.
fn slide_frames(width: f32, fold: bool) -> Vec<Vec<(String, f32, f32, f32)>> {
    let mut projects: Vec<ProjectLink> = (1..=9).map(|n| link(&format!("Project {n}"))).collect();
    let mut start = StartScreen::default();
    start.set_nav(StartNav {
        folded: !fold,
        ..Default::default()
    });
    let ctx = theme::test_context();
    ctx.enable_accesskit();
    let mut frames = Vec::new();
    let mut time = 0.0;
    for frame in 0..60 {
        if frame == 10 {
            start.set_nav(StartNav {
                folded: fold,
                ..Default::default()
            });
        }
        time += if frame < 10 { 1.0 } else { 1.0 / 60.0 };
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 1400.0))),
            time: Some(time),
            ..Default::default()
        };
        let output = crate::theme::run(&ctx, input, |ui| {
            let ctx = &ui.ctx().clone();
            theme::apply(ctx, &theme::Appearance::default());
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
                .show(ui, |ui| {
                    let brand = Brand::load(ui.ctx());
                    assert!(start.show(ui, &mut projects, &brand).is_none());
                });
        });
        if frame >= 9 {
            frames.push(drawn_buttons(&output));
        }
    }
    frames
}

#[test]
fn the_page_slides_with_the_sidebar_instead_of_reflowing() {
    for width in [1100.0, 1280.0, 1440.0, 1920.0] {
        for fold in [true, false] {
            let frames = slide_frames(width, fold);
            let cards = |frame: &[(String, f32, f32, f32)]| -> Vec<(f32, f32, f32)> {
                // Only in All projects, past the four in Recent.
                (5..=8)
                    .map(|n| {
                        let name = format!("Project {n}");
                        let (_, left, right, top) = frame
                            .iter()
                            .find(|(label, ..)| *label == name)
                            .unwrap_or_else(|| panic!("`{name}` is drawn"));
                        (*left, *right - *left, *top)
                    })
                    .collect()
            };
            let settled = cards(frames.last().unwrap());
            let mut last_left = cards(&frames[1])[0].0;
            // From the first frame of the slide, every card already has the
            // size and row it ends with; it only moves, one way, steadily.
            for (index, frame) in frames.iter().enumerate().skip(1) {
                let now = cards(frame);
                for ((_, size, top), (_, end_size, end_top)) in now.iter().zip(&settled) {
                    assert!(
                        (size - end_size).abs() < 0.5 && (top - end_top).abs() < 0.5,
                        "{size} wide at {top} on frame {index}, {end_size} at {end_top} at \
                         the end ({width} wide window, folding {fold})"
                    );
                }
                let left = now[0].0;
                let step = left - last_left;
                assert!(
                    if fold { step <= 0.5 } else { step >= -0.5 },
                    "frame {index} moved {step} the wrong way ({width} wide, folding {fold})"
                );
                assert!(step.abs() < 60.0, "frame {index} jumped {step}");
                last_left = left;
            }
        }
    }
}

/// A gradient picture for a project's thumbnail, in a new folder.
fn pictured_project(name: &str, tint: [u8; 3]) -> ProjectLink {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-start-{}-{}",
        name.replace(' ', "-"),
        std::process::id()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let mut rgb = Vec::new();
    for y in 0..180u32 {
        for x in 0..320u32 {
            let shade = |base: u8, along: u32, over: u32| {
                (u32::from(base) * (over / 2 + along) / over).min(255) as u8
            };
            rgb.extend([
                shade(tint[0], x, 320),
                shade(tint[1], 180 - y, 180),
                shade(tint[2], x / 2 + y, 340),
            ]);
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
    std::fs::write(folder.join(axcenity_project::THUMBNAIL_FILE), png_bytes).unwrap();
    ProjectLink {
        name: name.to_owned(),
        description: String::new(),
        folder,
    }
}

/// Screenshots of the start screen's look: at rest, a card leaning under
/// the pointer, part way through the sidebar folding, and the light theme.
/// Only with AXCENITY_SNAPSHOTS.
#[test]
fn the_start_screen_look_draws_at_rest_hovered_and_sliding() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) else {
        return;
    };
    let size = vec2(1440.0, 900.0);
    let mut projects = vec![
        pictured_project("Space Race", [90, 140, 250]),
        pictured_project("Lava Obby", [250, 120, 60]),
        pictured_project("Castle Builder", [120, 220, 140]),
        link("Night Market"),
    ];
    // Where the cards are, to point at them.
    let ctx = theme::test_context();
    ctx.enable_accesskit();
    let mut start = StartScreen::default();
    let mut output = None;
    for frame in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
            time: Some(frame as f64),
            ..Default::default()
        };
        output = Some(crate::theme::run(&ctx, input, |ui| {
            let ctx = &ui.ctx().clone();
            theme::apply(ctx, &theme::Appearance::default());
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    let brand = Brand::load(ui.ctx());
                    start.show(ui, &mut projects, &brand);
                });
        }));
    }
    let buttons = drawn_buttons(&output.unwrap());
    let corner_of = |name: &str| {
        let (_, left, right, top) = buttons
            .iter()
            .find(|(label, ..)| label == name)
            .unwrap_or_else(|| panic!("`{name}` is drawn"));
        pos2(left + (right - left) * 0.85, top + 30.0)
    };
    let pointers = [
        ("start-look-rest", None),
        ("start-look-hover-template", Some(corner_of("Empty"))),
        ("start-look-hover-game", Some(corner_of("Vehicle"))),
        ("start-look-hover-project", Some(corner_of("Lava Obby"))),
    ];
    for light in [false, true] {
        let appearance = theme::Appearance {
            theme: if light {
                theme::ThemeChoice::Light
            } else {
                theme::ThemeChoice::Dark
            },
            ..Default::default()
        };
        for (name, pointer) in pointers {
            if light && pointer.is_some() {
                continue;
            }
            let mut start = StartScreen::default();
            let mut frames: Vec<(f64, Vec<egui::Event>)> =
                (0..4).map(|i| (f64::from(i) * 0.5, Vec::new())).collect();
            if let Some(pointer) = pointer {
                for step in 0..30 {
                    frames.push((
                        2.0 + f64::from(step) / 60.0,
                        vec![egui::Event::PointerMoved(pointer)],
                    ));
                }
            }
            let file = if light {
                format!("{name}-light.png")
            } else {
                format!("{name}.png")
            };
            crate::snapshot::snapshot_timed(&folder.join(file), size, frames, |ui, _| {
                let ctx = &ui.ctx().clone();
                theme::apply(ctx, &appearance);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        let brand = Brand::load(ui.ctx());
                        start.show(ui, &mut projects, &brand);
                    });
            });
        }
    }
    // Part way through folding: a third of the slide in.
    for (name, stop) in [("start-look-folding", 0.08), ("start-look-folded", 1.0)] {
        let mut start = StartScreen::default();
        let mut frames: Vec<(f64, Vec<egui::Event>)> =
            (0..4).map(|i| (f64::from(i) * 0.5, Vec::new())).collect();
        frames.push((2.0, Vec::new()));
        frames.push((2.0 + stop, Vec::new()));
        let mut frame = 0;
        crate::snapshot::snapshot_timed(
            &folder.join(format!("{name}.png")),
            size,
            frames,
            |ui, _| {
                frame += 1;
                if frame == 5 {
                    start.set_nav(StartNav {
                        folded: true,
                        ..Default::default()
                    });
                }
                let ctx = &ui.ctx().clone();
                theme::apply(ctx, &theme::Appearance::default());
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        let brand = Brand::load(ui.ctx());
                        start.show(ui, &mut projects, &brand);
                    });
            },
        );
    }
}

#[test]
fn the_join_dialog_asks_for_a_link_then_says_how_joining_goes() {
    let mut projects = vec![link("Space Race")];
    let mut start = StartScreen::default();
    let ctx = theme::test_context();
    ctx.enable_accesskit();
    // The clock only goes forward, as it does in the editor.
    let mut clock = 0.0;
    let mut draw = |start: &mut StartScreen| {
        let mut output = None;
        for _ in 0..3 {
            clock += 10.0;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
                time: Some(clock),
                ..Default::default()
            };
            output = Some(crate::theme::run(&ctx, input, |ui| {
                let ctx = &ui.ctx().clone();
                theme::apply(ctx, &theme::Appearance::default());
                egui::CentralPanel::default().show(ui, |ui| {
                    let brand = Brand::load(ui.ctx());
                    assert!(start.show(ui, &mut projects, &brand).is_none());
                });
            }));
        }
        drawn_text(&output.unwrap())
    };
    // The header offers Join beside New and Open.
    let drawn = draw(&mut start);
    find(&drawn, "Join…");

    start.join_session();
    let drawn = draw(&mut start);
    find(&drawn, "Join a Collaborate session");
    find(&drawn, "Invite link");

    start.show_joining();
    start.set_join_progress(Some(JoinProgress::Fetching {
        host: "192.168.1.20:47800".to_owned(),
    }));
    let drawn = draw(&mut start);
    find(&drawn, "Fetching the session's files");
    assert!(!drawn.iter().any(|(text, ..)| text == "Invite link"));

    // Joining stopped (the project opened, or the session ended): the
    // dialog closes by itself.
    start.set_join_progress(None);
    draw(&mut start);
    assert!(start.join_dialog().is_none());

    start.join_failed("this invite was already used".to_owned());
    let drawn = draw(&mut start);
    find(&drawn, "this invite was already used");

    // A link the system handed over is only offered: it says whose session
    // it is and waits for Join (C4).
    start.offer_join(
        "axcenity://collaborate/join?host=pauls-pc.tail1.ts.net:47810&cert=".to_owned()
            + &"ab".repeat(32)
            + "&invite="
            + &"cd".repeat(16)
            + "&name=Castle%20Game",
    );
    assert_eq!(start.join_dialog(), Some((false, None, None)));
    let drawn = draw(&mut start);
    find(
        &drawn,
        "“Castle Game”, hosted by pauls-pc.tail1.ts.net:47810",
    );
    find(
        &drawn,
        "Join only if you expected this invite. It's reached over Tailscale",
    );
}
