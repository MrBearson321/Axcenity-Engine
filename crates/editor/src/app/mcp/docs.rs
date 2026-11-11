//! `docs`: the engine's own reference, on demand, so tool descriptions stay
//! one sentence. It reads the same text as the script API reference and
//! the Properties panel's descriptions (`script::api_reference`,
//! `property_description`), so it can't drift from the engine.

use std::sync::OnceLock;

use axcenity_core::ClassName;
use axcenity_mcp::{
    Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec, closest,
};
use axcenity_script::{ApiEntry, ApiKind};

use super::Host;
use super::LIMIT;
use super::refs::quoted;
use super::values::{show, type_name};

pub const SPEC: ToolSpec = ToolSpec {
    name: "docs",
    title: "Engine docs",
    summary: "Look up a class's properties, one property, or search the script API.",
    profile: Profile::Lite,
    class: Class::Read,
    idempotent: true,
    params: &[
        Param::optional(
            "class",
            Kind::String,
            "a class name, such as Part or PointLight",
        ),
        Param::optional("property", Kind::String, "one property of class, in full"),
        Param::optional("query", Kind::String, "words to search the script API for"),
        LIMIT,
    ],
};

fn reference() -> &'static [ApiEntry] {
    static REFERENCE: OnceLock<Vec<ApiEntry>> = OnceLock::new();
    REFERENCE.get_or_init(axcenity_script::api_reference)
}

/// The first sentence of `text`, at most `max` characters.
fn first_sentence(text: &str, max: usize) -> String {
    let sentence = match text.find(". ") {
        Some(end) => &text[..=end],
        None => text,
    };
    let sentence = sentence.trim();
    if sentence.chars().count() > max {
        let cut: String = sentence.chars().take(max - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        sentence.to_owned()
    }
}

pub fn call(_host: &mut Host, args: &Args) -> Outcome {
    if let Some(name) = args.str("class") {
        // Services and value types the scene never holds (RunService,
        // Vector3) are only in the script API reference.
        if ClassName::from_name(name).is_none()
            && let Some(page) = api_page(name, args.str("property"))
        {
            return Ok(page);
        }
        let class = find_class(name)?;
        return match args.str("property") {
            Some(property) => Ok(property_page(class, property)?),
            None => Ok(class_page(class)),
        };
    }
    if args.str("property").is_some() {
        return Err(ToolError::bad_args("property needs class too")
            .with_fix("give class, such as {\"class\":\"Part\",\"property\":\"Size\"}"));
    }
    if let Some(query) = args.str("query") {
        return Ok(search(query, args.limit(8, 30)));
    }
    let classes: Vec<&str> = ClassName::ALL.iter().map(|class| class.as_str()).collect();
    Ok(format!(
        "docs: give class (its properties), class + property, or query (search the script API).\nclasses: {}\nobject_create kinds: {}\nscripts: script_write makes and changes them",
        classes.join(", "),
        super::object_create::offered().join(", ")
    ))
}

/// A class, type or library of the script API (`RunService`, `Vector3`,
/// `task`) and its members, or one member: `None` if there's none called
/// `name`.
fn api_page(name: &str, member: Option<&str>) -> Option<String> {
    let entries = reference();
    let owner = entries.iter().find(|entry| {
        entry.owner.is_empty()
            && entry.name == name
            && matches!(
                entry.kind,
                ApiKind::Class | ApiKind::Type | ApiKind::Library
            )
    })?;
    let members: Vec<&ApiEntry> = entries
        .iter()
        .filter(|entry| entry.owner == owner.name)
        .collect();
    if let Some(member) = member {
        let found = members.iter().find(|entry| entry.name == member)?;
        let mut text = format!("{} {}", found.kind.label(), found.signature);
        if !found.doc.is_empty() {
            text.push_str(&format!("\n{}", found.doc));
        }
        return Some(text);
    }
    let mut text = owner.name.clone();
    if !owner.doc.is_empty() {
        text.push_str(&format!(": {}", first_sentence(&owner.doc, 200)));
    }
    for entry in members {
        text.push_str(&format!("\n  {} {}", entry.kind.label(), entry.signature));
        if !entry.doc.is_empty() {
            text.push_str(&format!(" · {}", first_sentence(&entry.doc, 80)));
        }
    }
    Some(text)
}

fn find_class(name: &str) -> Result<ClassName, ToolError> {
    ClassName::from_name(name).ok_or_else(|| {
        let names: Vec<&str> = ClassName::ALL.iter().map(|class| class.as_str()).collect();
        let near = closest(name, names, 3);
        let error = ToolError::new(Code::NotFound, format!("no class {}", quoted(name)));
        if near.is_empty() {
            error.with_fix("docs with no arguments lists every class")
        } else {
            error.with_fix(format!("did you mean {}?", near.join(" or ")))
        }
    })
}

fn class_page(class: ClassName) -> String {
    let doc = reference()
        .iter()
        .find(|entry| entry.kind == ApiKind::Class && entry.name == class.as_str())
        .map(|entry| first_sentence(&entry.doc, 200))
        .unwrap_or_default();
    let mut line = class.as_str().to_owned();
    let supers = class.superclasses();
    if !supers.is_empty() {
        line.push_str(&format!(" ({})", supers.join(", ")));
    }
    if !doc.is_empty() {
        line.push_str(&format!(": {doc}"));
    }
    let mut text = line;
    if super::object_create::is_script_class(class.as_str()) {
        text.push_str(&format!(
            "\nto make one: {}",
            super::object_create::SCRIPT_FIX
        ));
    }
    let mut properties = class.default_properties();
    properties.sort_by_key(|(name, _)| *name);
    text.push_str("\nproperties (name type default · meaning):");
    for (name, value) in properties {
        let meaning = axcenity_script::property_description(class, name)
            .map(|doc| format!(" · {}", first_sentence(doc, 90)))
            .unwrap_or_default();
        text.push_str(&format!(
            "\n  {name} {} {}{meaning}",
            type_name(&value),
            show(&value)
        ));
    }
    let events = class.events();
    if !events.is_empty() {
        text.push_str(&format!("\nevents: {}", events.join(", ")));
    }
    text
}

fn property_page(class: ClassName, property: &str) -> Result<String, ToolError> {
    let properties = class.default_properties();
    let Some((name, value)) = properties.iter().find(|(name, _)| *name == property) else {
        let names: Vec<&str> = properties.iter().map(|(name, _)| *name).collect();
        let near = closest(property, names, 3);
        let error = ToolError::new(
            Code::UnknownProperty,
            format!("{} has no property {}", class.as_str(), quoted(property)),
        );
        return Err(if near.is_empty() {
            error.with_fix(format!(
                "docs with class \"{}\" lists its properties",
                class.as_str()
            ))
        } else {
            error.with_fix(format!("did you mean {}?", near.join(" or ")))
        });
    };
    let mut text = format!(
        "{}.{name}: {} · default {}",
        class.as_str(),
        type_name(value),
        show(value)
    );
    if let Some(doc) = axcenity_script::property_description(class, name) {
        text.push_str(&format!("\n{doc}"));
    }
    let kind = type_name(value);
    let choices: Vec<&str> = reference()
        .iter()
        .filter(|entry| {
            entry.kind == ApiKind::EnumItem
                && (entry.owner == kind || entry.owner == format!("Enum.{kind}"))
        })
        .map(|entry| entry.name.as_str())
        .collect();
    if !choices.is_empty() {
        text.push_str(&format!("\nchoices: {}", choices.join(", ")));
    }
    Ok(text)
}

fn search(query: &str, limit: usize) -> String {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut scored: Vec<(usize, &ApiEntry)> = reference()
        .iter()
        .filter_map(|entry| {
            let name = entry.qualified_name().to_lowercase();
            let doc = entry.doc.to_lowercase();
            let mut score = 0;
            for word in &words {
                if name.ends_with(word.as_str()) {
                    score += 4;
                } else if name.contains(word.as_str()) {
                    score += 3;
                } else if doc.contains(word.as_str()) {
                    score += 1;
                } else {
                    return None;
                }
            }
            Some((score, entry))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.qualified_name().cmp(&b.1.qualified_name()))
    });
    if scored.is_empty() {
        return format!("nothing in the script API matches {}", quoted(query));
    }
    let mut text = format!("{} matches for {}", scored.len(), quoted(query));
    for (_, entry) in scored.iter().take(limit) {
        text.push_str(&format!(
            "\n{} {}{} · {}",
            entry.kind.label(),
            entry.qualified_name(),
            if entry.signature.is_empty() {
                String::new()
            } else {
                format!(" {}", entry.signature)
            },
            first_sentence(&entry.doc, 100)
        ));
    }
    if scored.len() > limit {
        text.push_str(&format!(
            "\n… {} more; narrow the query or raise limit",
            scored.len() - limit
        ));
    }
    text
}
