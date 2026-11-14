//! The API Reference as Markdown pages, for `documentation/api/` (the
//! GitBook site): one page per class and type, one each for the enums,
//! libraries and globals, and their part of `documentation/SUMMARY.md`.
//! Written from [`api_reference`] and [`api_example`], so the pages say what
//! the editor's API Reference says. `cargo run -p axcenity-script --bin
//! lua-definitions` writes them, and `api_pages_are_up_to_date` checks.

use super::{ApiEntry, ApiKind, api_reference, examples::api_example};

/// Where the pages go, from the repository's root.
pub const API_DOCS_PATH: &str = "documentation/api";
/// The summary the API section ends, from the repository's root.
pub const SUMMARY_PATH: &str = "documentation/SUMMARY.md";
/// The heading the generated part of the summary starts with. Everything
/// from it to the end of `SUMMARY.md` is generated.
pub const SUMMARY_HEADING: &str = "## API reference";

/// The kinds of member a page lists, in order, with their headings.
const MEMBERS: [(ApiKind, &str); 4] = [
    (ApiKind::Property, "Properties"),
    (ApiKind::Event, "Events"),
    (ApiKind::Function, "Functions"),
    (ApiKind::EnumItem, "Items"),
];

/// Every page: `(path inside documentation/api, Markdown)`, in a fixed
/// order.
pub fn api_markdown() -> Vec<(String, String)> {
    let entries = api_reference();
    let top = |kind: ApiKind| -> Vec<&ApiEntry> {
        entries
            .iter()
            .filter(|entry| entry.owner.is_empty() && entry.kind == kind)
            .collect()
    };
    let classes = top(ApiKind::Class);
    let types = top(ApiKind::Type);
    let mut pages = vec![("README.md".to_owned(), readme())];
    for (folder, title, what, list) in [
        (
            "classes",
            "Classes",
            "Instance classes: everything that can be in the tree.",
            &classes,
        ),
        (
            "types",
            "Data types",
            "Values that aren't Instances, such as Vector3, CFrame and Signal.",
            &types,
        ),
    ] {
        pages.push((format!("{folder}.md"), index(title, what, folder, list)));
        for entry in list.iter() {
            pages.push((
                format!("{folder}/{}.md", entry.name),
                entry_page(&entries, entry),
            ));
        }
    }
    pages.push((
        "enums.md".to_owned(),
        grouped(
            &entries,
            "Enums",
            "Every enum and its items, such as `Enum.Material.Neon`.",
            &top(ApiKind::Enum),
        ),
    ));
    pages.push((
        "libraries.md".to_owned(),
        grouped(
            &entries,
            "Libraries",
            "Global tables of functions, such as `task`.",
            &top(ApiKind::Library),
        ),
    ));
    let mut globals = top(ApiKind::Global);
    globals.extend(
        entries
            .iter()
            .filter(|entry| entry.owner.is_empty() && entry.kind == ApiKind::Function),
    );
    pages.push((
        "globals.md".to_owned(),
        grouped(
            &entries,
            "Globals",
            "The variables and functions every script has.",
            &globals,
        ),
    ));
    pages
}

/// The API section of `SUMMARY.md`, from [`SUMMARY_HEADING`] to the end.
pub fn api_summary() -> String {
    let entries = api_reference();
    let mut text = format!("{SUMMARY_HEADING}\n\n* [API reference](api/README.md)\n");
    for (folder, title, kind) in [
        ("classes", "Classes", ApiKind::Class),
        ("types", "Data types", ApiKind::Type),
    ] {
        text.push_str(&format!("  * [{title}](api/{folder}.md)\n"));
        for entry in entries
            .iter()
            .filter(|entry| entry.owner.is_empty() && entry.kind == kind)
        {
            text.push_str(&format!("    * [{0}](api/{folder}/{0}.md)\n", entry.name));
        }
    }
    text.push_str("  * [Enums](api/enums.md)\n");
    text.push_str("  * [Libraries](api/libraries.md)\n");
    text.push_str("  * [Globals](api/globals.md)\n");
    text
}

/// `SUMMARY.md`'s text with its API section replaced by [`api_summary`]
/// (added at the end if it has none).
pub fn with_api_summary(summary: &str) -> String {
    let summary = summary.replace("\r\n", "\n");
    let before = match summary.find(SUMMARY_HEADING) {
        Some(at) => &summary[..at],
        None => summary.as_str(),
    };
    format!("{}\n\n{}", before.trim_end(), api_summary())
}

/// The folder's front page. It names no counts on purpose: every API change
/// would rewrite the same line, so two open pull requests that each add to
/// the API would always conflict here.
fn readme() -> String {
    "---\ndescription: Every class, data type, enum, library and global the engine's \
         scripts can use, generated from the engine itself.\n---\n\n\
         # API reference\n\n\
         Everything Luau scripts can use: every class, data type, property, event, \
         function and enum item, with what each does and usage examples for the most \
         used ones.\n\n\
         These pages are generated from the engine's own API description, the one the \
         editor's **API Reference** page and the VS Code definitions \
         (`definitions/axcenity.d.luau`) come from, so they always match the engine. \
         Don't edit them by hand: run `cargo run -p axcenity-script --bin lua-definitions` \
         after changing the API.\n\n\
         * [Classes](classes.md): parts, models, lights, sounds, UI, scripts and \
         everything else in the tree.\n\
         * [Data types](types.md): Vector3, CFrame, Color3, UDim2, Signal and the \
         services that aren't Instances.\n\
         * [Enums](enums.md): fixed lists of choices.\n\
         * [Libraries](libraries.md): global tables of functions, such as `task`.\n\
         * [Globals](globals.md): `game`, `workspace`, `script`, `require` and the rest.\n\n\
         Every class also has the members of [Instance](classes/Instance.md).\n"
        .to_owned()
}

/// A list of `entries`, each linked to its page in `folder`.
fn index(title: &str, what: &str, folder: &str, entries: &[&ApiEntry]) -> String {
    let mut text = format!(
        "---\ndescription: {}\n---\n\n# {title}\n\n{what}\n\n| Name | What it is |\n|---|---|\n",
        plain(what)
    );
    for entry in entries {
        text.push_str(&format!(
            "| [{0}]({folder}/{0}.md) | {1} |\n",
            entry.name,
            cell(&first_sentence(&entry.doc))
        ));
    }
    text
}

/// One class's or type's page: what it is, then its members.
fn entry_page(entries: &[ApiEntry], entry: &ApiEntry) -> String {
    let summary = first_sentence(&entry.doc);
    let mut text = format!(
        "---\ndescription: {}\n---\n\n# {}\n\n",
        if summary.is_empty() {
            format!("The {} {}.", entry.name, entry.kind.label().to_lowercase())
        } else {
            plain(&summary)
        },
        entry.name
    );
    text.push_str(&describe(entry));
    if entry.kind == ApiKind::Class && entry.name != "Instance" {
        text.push_str("It also has every member of [Instance](Instance.md).\n\n");
    }
    members(entries, &entry.name, &mut text);
    text
}

/// A page of several top-level entries, each a section with its members.
fn grouped(entries: &[ApiEntry], title: &str, what: &str, tops: &[&ApiEntry]) -> String {
    let mut text = format!(
        "---\ndescription: {}\n---\n\n# {title}\n\n{what}\n\n",
        plain(what)
    );
    for entry in tops {
        text.push_str(&format!("## {}\n\n", entry.name));
        text.push_str(&describe(entry));
        if entry.kind == ApiKind::Enum {
            // Enum items share one description, so they're listed as a line.
            let items: Vec<String> = entries
                .iter()
                .filter(|item| item.owner == entry.name && item.kind == ApiKind::EnumItem)
                .map(|item| format!("`{}`", item.name))
                .collect();
            text.push_str(&format!("Items: {}\n\n", items.join(", ")));
        } else {
            members(entries, &entry.name, &mut text);
        }
    }
    text
}

/// An entry's signature, description and example.
fn describe(entry: &ApiEntry) -> String {
    let mut text = String::new();
    if entry.signature != entry.name {
        text.push_str(&format!("```luau\n{}\n```\n\n", entry.signature));
    }
    if !entry.doc.is_empty() {
        text.push_str(&format!("{}\n\n", entry.doc));
    }
    if let Some(example) = api_example(&entry.qualified_name()) {
        text.push_str(&format!("```lua\n{example}\n```\n\n"));
    }
    text
}

/// The members of `owner`, by kind, each with its own heading.
fn members(entries: &[ApiEntry], owner: &str, text: &mut String) {
    for (kind, heading) in MEMBERS {
        let list: Vec<&ApiEntry> = entries
            .iter()
            .filter(|entry| entry.owner == owner && entry.kind == kind)
            .collect();
        if list.is_empty() {
            continue;
        }
        text.push_str(&format!("### {heading}\n\n"));
        for member in list {
            text.push_str(&format!("#### {}\n\n", member.name));
            text.push_str(&describe(member));
        }
    }
}

/// The first sentence of a description.
fn first_sentence(doc: &str) -> String {
    match doc.find(". ") {
        Some(end) => doc[..=end].to_owned(),
        None => doc.to_owned(),
    }
}

/// Text for a table cell: no `|` to end it early, no line breaks.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// Text for a front-matter line: one line without backticks, quoted for
/// YAML.
fn plain(text: &str) -> String {
    let text = text.replace(['`', '\n'], "");
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}
