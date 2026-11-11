//! Checks on the documentation in `documentation/`: it reads cleanly, every
//! link and sample is real, every API name in backticks exists, and every
//! Lua example compiles. These keep the pages true as the engine changes:
//! renaming or removing an API, a sample or a page fails here until the
//! pages say so. See `docs/editor/learn-and-docs.md`.

use std::collections::HashSet;

use super::library::{Block, LinkTarget, Page};
use super::markdown::{self, ICON_NAMES};
use super::*;
use crate::start::{Page as StartPage, learn};

/// Code-block languages the pages may use.
const LANGUAGES: &[&str] = &[
    "lua",
    "luau",
    "json",
    "css",
    "text",
    "sh",
    "powershell",
    "toml",
];

fn pages() -> &'static [Page] {
    &library().pages
}

#[test]
fn the_documentation_reads_without_problems() {
    let library = library();
    assert!(library.pages.len() > 10, "documentation/ is built in");
    let mut problems = library.problems.clone();
    for page in pages() {
        problems.extend(page.problems.iter().map(|p| format!("{}: {p}", page.path)));
        if page.description.is_empty() {
            problems.push(format!("{}: no `description:` line", page.path));
        }
        if !page.icon.is_empty() && !ICON_NAMES.contains(&page.icon.as_str()) {
            problems.push(format!(
                "{}: icon `{}` isn't one of {ICON_NAMES:?}",
                page.path, page.icon
            ));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn every_link_goes_somewhere() {
    let library = library();
    let mut broken = Vec::new();
    for page in pages() {
        let anchors = |page: &Page| -> HashSet<String> {
            page.headings()
                .map(|(_, _, anchor)| anchor.to_owned())
                .collect()
        };
        for target in page.links() {
            match library.resolve(&page.path, target) {
                LinkTarget::Missing(path) => {
                    broken.push(format!("{}: `{target}` ({path} isn't a page)", page.path));
                }
                LinkTarget::Anchor(anchor) if !anchors(page).contains(&anchor) => {
                    broken.push(format!("{}: no heading for `#{anchor}`", page.path));
                }
                LinkTarget::Page(index, Some(anchor))
                    if !anchors(&library.pages[index]).contains(&anchor) =>
                {
                    broken.push(format!("{}: `{target}` has no such heading", page.path));
                }
                LinkTarget::Sample(name) if learn::find_sample(&name).is_none() => {
                    broken.push(format!("{}: examples/{name} doesn't exist", page.path));
                }
                _ => {}
            }
        }
    }
    assert!(broken.is_empty(), "{broken:#?}");
}

#[test]
fn every_sample_a_page_follows_opens() {
    for page in pages() {
        for sample in &page.samples {
            let folder = learn::find_sample(sample)
                .unwrap_or_else(|| panic!("{}: examples/{sample} wasn't found", page.path));
            axcenity_project::load(&folder)
                .unwrap_or_else(|err| panic!("{}: examples/{sample}: {err}", page.path));
        }
    }
}

/// `Part.Touched`, `Instance:FindFirstChild()`, `task.wait` or
/// `Enum.Material.Neon` with an owner the API Reference knows: then the
/// member has to exist too.
fn named_member(code: &str) -> Option<(String, String)> {
    let name = code.split('(').next()?.trim().replace(':', ".");
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return None;
    }
    let name = name.strip_prefix("Enum.").unwrap_or(&name);
    let (owner, member) = name.split_once('.')?;
    if member.contains('.') || member.is_empty() {
        return None;
    }
    // `game.axgame` is a file, not a member of `game`.
    let owner = match markdown::global_class(owner) {
        Some(_) if member.starts_with(char::is_lowercase) => return None,
        Some(class) => class,
        None => owner,
    };
    markdown::api_names()
        .contains(owner)
        .then(|| (owner.to_owned(), member.to_owned()))
}

#[test]
fn every_api_name_in_backticks_exists() {
    let mut missing = Vec::new();
    let mut code_spans = 0;
    for page in pages() {
        for code in page.code_spans() {
            code_spans += 1;
            if let Some((owner, member)) = named_member(code)
                && markdown::api_name(code).is_none()
            {
                missing.push(format!(
                    "{}: `{code}`: {owner} has no {member} in the API Reference",
                    page.path
                ));
            }
        }
    }
    assert!(code_spans > 0);
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn api_names_link_to_their_entries() {
    assert_eq!(markdown::api_name("Part").as_deref(), Some("Part"));
    assert_eq!(
        markdown::api_name("Part.Touched").as_deref(),
        Some("Part.Touched")
    );
    assert_eq!(
        markdown::api_name("Instance:FindFirstChild(name)").as_deref(),
        Some("Instance.FindFirstChild")
    );
    assert_eq!(
        markdown::api_name("task.wait()").as_deref(),
        Some("task.wait")
    );
    assert_eq!(markdown::api_name("world.json"), None);
    assert_eq!(markdown::api_name("F5"), None);
    assert_eq!(
        markdown::api_name("workspace:Raycast(origin, direction)").as_deref(),
        Some("Workspace.Raycast")
    );
    assert!(named_member("Part.NoSuchThing").is_some());
    assert!(named_member("world.json").is_none());
    assert!(named_member("game.axgame").is_none());
}

#[test]
fn every_lua_example_compiles_and_every_code_block_names_its_language() {
    let mut problems = Vec::new();
    let mut examples = 0;
    for page in pages() {
        for (number, block) in page.blocks.iter().enumerate() {
            let Block::Code { lang, text } = block else {
                continue;
            };
            if !LANGUAGES.contains(&lang.as_str()) {
                problems.push(format!(
                    "{}: code block {number} has language `{lang}`, not one of {LANGUAGES:?}",
                    page.path
                ));
            }
            if matches!(lang.as_str(), "lua" | "luau") {
                examples += 1;
                if let Some(error) = axcenity_script::syntax_error(&page.path, text) {
                    problems.push(format!("{}: {error}\n{text}", page.path));
                }
            }
        }
    }
    assert!(examples > 0);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_learning_path_covers_each_part_of_the_engine() {
    let library = library();
    let chapters = learn::chapters(library);
    let titles: Vec<&str> = chapters
        .iter()
        .map(|&(_, page)| library.pages[page].title.as_str())
        .collect();
    // The topics Paul's brief (2026-11-11) asked the path to cover.
    for topic in [
        "Getting started",
        "Editor",
        "Projects",
        "Building",
        "Explorer",
        "Materials",
        "Lighting",
        "Physics",
        "Animation",
        "Audio",
        "Terrain",
        "UI",
        "Scripting",
        "Blocks",
        "Assets",
        "Debugging",
        "Publishing",
    ] {
        assert!(
            titles.iter().any(|title| title.contains(topic)),
            "no chapter about {topic}: {titles:?}"
        );
    }
    for &(_, page) in &chapters {
        let page = &library.pages[page];
        assert!(!page.icon.is_empty(), "{} has an icon", page.path);
    }
}

#[test]
fn search_finds_titles_before_text() {
    let library = Library::load(&[
        (
            "SUMMARY.md",
            "# Table of contents\n\n## Things\n* [A](a.md)\n* [B](b.md)\n",
        ),
        (
            "a.md",
            "---\ndescription: First.\n---\n# Physics\n\nParts fall.\n",
        ),
        (
            "b.md",
            "---\ndescription: Second.\n---\n# Scripts\n\n## Gravity\n\nPhysics pulls parts down.\n",
        ),
    ]);
    assert!(library.problems.is_empty(), "{:?}", library.problems);
    let hits = library.search("physics");
    assert_eq!(hits.len(), 2);
    assert_eq!(library.pages[hits[0].page].title, "Physics");
    assert_eq!(hits[1].anchor.as_deref(), Some("gravity"));
    assert!(hits[1].snippet.contains("Physics pulls"));
    assert!(library.search("nothing like this").is_empty());
}

#[test]
fn markdown_reads_into_blocks() {
    let page = Page::parse(
        "learn/a.md",
        "---\ndescription: D.\nstatus: experimental\nsamples: [hello-part, hello-ui]\nicon: cube\n---\n\
         # Title\n\nSome **bold** and `code` and [a link](../b.md#x).\n\n\
         ## Next `step`\n\n- one\n- two\n  wrapped\n\n1. first\n2. second\n\n\
         > A note.\n\n```lua\nprint(1)\n```\n\n| A | B |\n|---|---|\n| `x\\|y` | 2 |\n",
    );
    assert!(page.problems.is_empty(), "{:?}", page.problems);
    assert_eq!(page.title, "Title");
    assert_eq!(page.status, library::Status::Experimental);
    assert_eq!(page.samples, ["hello-part", "hello-ui"]);
    assert_eq!(page.links(), ["../b.md#x"]);
    assert_eq!(
        page.headings().collect::<Vec<_>>(),
        [(2, "Next step", "next-step")]
    );
    assert!(matches!(&page.blocks[2], Block::List { ordered: false, items } if items.len() == 2));
    assert!(matches!(&page.blocks[3], Block::List { ordered: true, .. }));
    assert!(matches!(&page.blocks[4], Block::Note(_)));
    assert!(matches!(&page.blocks[5], Block::Code { lang, .. } if lang == "lua"));
    assert!(matches!(&page.blocks[6], Block::Table { rows, .. } if rows.len() == 1));
    assert_eq!(page.code_spans(), ["code", "x|y"]);
    assert_eq!(library::join("learn/a.md", "../b.md"), "b.md");

    let bad = Page::parse(
        "x.md",
        "# T\n\n- a\n  - nested\n\n<div>html</div>\n\nan `unclosed\n",
    );
    assert_eq!(bad.problems.len(), 3, "{:?}", bad.problems);
}

#[test]
fn the_documentation_page_draws_every_page_wide_and_narrow() {
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    let library = library();
    for index in 0..library.pages.len() {
        let learn_page = learn::chapters(library)
            .iter()
            .any(|&(_, page)| page == index);
        for (width, suffix) in [(1400.0, ""), (520.0, "-narrow")] {
            let mut start = StartScreen {
                page: if learn_page {
                    StartPage::Learn
                } else {
                    StartPage::Docs
                },
                ..Default::default()
            };
            if learn_page {
                start.learn.open_chapter(index, None);
            } else {
                start.docs.open(index, None);
            }
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
            let name = library.pages[index].path.replace(['/', '.'], "-");
            match &snapshots {
                Some(folder) => crate::snapshot::snapshot(
                    &folder.join(format!("docs-{name}{suffix}.png")),
                    vec2(width, 2400.0),
                    vec![Vec::new(); 3],
                    &mut draw,
                ),
                _ => {
                    let ctx = theme::test_context();
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(width, 1600.0),
                        )),
                        ..Default::default()
                    };
                    for _ in 0..3 {
                        let _ = crate::theme::run(&ctx, input.clone(), |ui| draw(ui));
                    }
                }
            }
            if learn_page {
                assert_eq!(
                    start.learn.chapter,
                    Some(index),
                    "drawing alone goes nowhere"
                );
            } else {
                assert_eq!(start.docs.open, Some(index), "drawing alone goes nowhere");
            }
        }
    }
}

#[test]
fn searching_the_documentation_draws_results() {
    let mut start = StartScreen {
        page: StartPage::Docs,
        ..Default::default()
    };
    start.docs.query = "physics".to_owned();
    assert!(!library().search("physics").is_empty());
    let mut projects = Vec::new();
    let ctx = theme::test_context();
    for _ in 0..3 {
        let _ = crate::theme::run(&ctx, Default::default(), |ui| {
            let ctx = &ui.ctx().clone();
            theme::apply(ctx, &theme::Appearance::default());
            let brand = Brand::load(ctx);
            assert!(start.show(ui, &mut projects, &brand).is_none());
        });
    }
}
