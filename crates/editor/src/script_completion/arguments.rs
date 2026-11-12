//! Completion inside a string that is a call's first argument: the names a
//! method takes as text, such as `game:GetService("|")`,
//! `part:GetAttribute("|")`, `folder:WaitForChild("|")` or
//! `Instance.new("|")`. Offered from the API (services, classes,
//! properties) and the world being edited (children, attributes, tags).

use std::collections::BTreeSet;

use axcenity_core::{InstanceId, World};
use axcenity_script::ApiKind;

use crate::lua_syntax::TokenKind;

use super::{
    Api, Completions, Item, ItemKind, MAX_ITEMS, Resolver, Ty, api, is_word, receiver_start,
};

/// What a string argument names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Takes {
    Service,
    Class,
    /// A class or one of the abstract classes (`BasePart`), for `IsA`.
    Kind,
    Property,
    Attribute,
    Child,
    Descendant,
    Ancestor,
    Tag,
}

/// The method (or function) whose first argument is a name of this kind.
fn takes(method: &str, colon: bool, receiver: &str) -> Option<Takes> {
    if !colon {
        return (receiver == "Instance" && method == "new").then_some(Takes::Class);
    }
    Some(match method {
        "GetService" | "FindService" => Takes::Service,
        "FindFirstChildOfClass" | "FindFirstAncestorOfClass" | "FindFirstDescendantOfClass" => {
            Takes::Class
        }
        "IsA" | "FindFirstChildWhichIsA" | "FindFirstAncestorWhichIsA" => Takes::Kind,
        "GetPropertyChangedSignal" => Takes::Property,
        "GetAttribute" | "SetAttribute" | "GetAttributeChangedSignal" => Takes::Attribute,
        "FindFirstChild" | "WaitForChild" => Takes::Child,
        "FindFirstDescendant" => Takes::Descendant,
        "FindFirstAncestor" => Takes::Ancestor,
        "HasTag"
        | "AddTag"
        | "RemoveTag"
        | "GetTagged"
        | "GetInstanceAddedSignal"
        | "GetInstanceRemovedSignal" => Takes::Tag,
        _ => return None,
    })
}

/// The completions at byte `cursor` when it's inside a string that is the
/// first argument of a call taking a name. `None` anywhere else.
pub(super) fn complete(source: &str, cursor: usize, world: &World) -> Option<Completions> {
    let before = &source[..cursor];
    // In a string, not a comment.
    let in_string = crate::lua_syntax::tokens(before)
        .pop()
        .is_some_and(|(kind, range)| kind == TokenKind::String && range.end == cursor);
    if !in_string {
        return None;
    }
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    let line = &before[line_start..];
    // The quote the string starts with, and what's typed after it.
    let quote_at = line.rfind(['"', '\''])?;
    let quote = line[quote_at..].chars().next()?;
    let typed = &line[quote_at + 1..];
    if typed.contains('\\') {
        return None;
    }
    // `name(` just before the quote, with spaces allowed.
    let call = line[..quote_at].trim_end().strip_suffix('(')?.trim_end();
    let name_start = call
        .rfind(|c: char| !is_word(c))
        .map_or(0, |at| at + c_len(call, at));
    let method = &call[name_start..];
    let access = call[..name_start].chars().next_back()?;
    if !matches!(access, '.' | ':') || method.is_empty() {
        return None;
    }
    // The receiver, in the whole source's bytes.
    let access_at = line_start + name_start - 1;
    let receiver_from = receiver_start(source, access_at);
    let receiver = &source[receiver_from..access_at];
    let kind = takes(method, access == ':', receiver)?;
    let api = api();
    let ty = || Resolver::new(source, cursor, world).chain(receiver);
    let names: Vec<(String, String)> = match kind {
        Takes::Service => services(api, world),
        Takes::Class => api
            .classes
            .iter()
            .map(|class| (class.clone(), "class".to_owned()))
            .collect(),
        Takes::Kind => api
            .classes
            .iter()
            .map(|class| (class.clone(), "class".to_owned()))
            .chain(
                axcenity_core::ClassName::ALL
                    .iter()
                    .flat_map(|class| class.superclasses())
                    .chain(["Instance"].iter())
                    .map(|name| ((*name).to_owned(), "abstract class".to_owned())),
            )
            .collect(),
        Takes::Property => match ty() {
            Some(Ty::Instance { class, .. }) => api
                .members
                .get(class.as_str())
                .into_iter()
                .chain(api.members.get("Instance"))
                .flatten()
                .filter(|entry| entry.kind == ApiKind::Property)
                .map(|entry| (entry.name.clone(), entry.signature.clone()))
                .collect(),
            _ => return None,
        },
        Takes::Attribute => attributes(world, instance_of(ty())),
        Takes::Child | Takes::Descendant | Takes::Ancestor => {
            let id = instance_of(ty())?;
            let found: Vec<InstanceId> = match kind {
                Takes::Child => world
                    .get(id)
                    .map(|parent| parent.children().to_vec())
                    .unwrap_or_default(),
                Takes::Descendant => world
                    .descendants(id)
                    .into_iter()
                    .filter(|&found| found != id)
                    .collect(),
                _ => std::iter::successors(world.get(id).and_then(|it| it.parent()), |&up| {
                    world.get(up).and_then(|it| it.parent())
                })
                .collect(),
            };
            found
                .into_iter()
                .filter_map(|found| world.get(found))
                .map(|found| (found.name().to_owned(), found.class().to_string()))
                .collect()
        }
        Takes::Tag => world
            .all_tags()
            .into_iter()
            .map(|(tag, count)| (tag, format!("tag on {count}")))
            .collect(),
    };
    let lower = typed.to_lowercase();
    let mut seen = BTreeSet::new();
    let mut items: Vec<Item> = names
        .into_iter()
        .filter(|(name, _)| !name.contains(quote) && name.to_lowercase().starts_with(&lower))
        .filter(|(name, _)| seen.insert(name.clone()))
        .map(|(name, detail)| Item {
            label: name,
            kind: ItemKind::Name,
            detail,
            doc: String::new(),
        })
        .collect();
    items.sort_by(|a, b| {
        (!a.label.starts_with(typed), a.label.to_lowercase())
            .cmp(&(!b.label.starts_with(typed), b.label.to_lowercase()))
    });
    items.truncate(MAX_ITEMS);
    if items.is_empty() || (items.len() == 1 && items[0].label == typed) {
        return None;
    }
    Some(Completions {
        start: line_start + quote_at + 1,
        items,
    })
}

fn c_len(text: &str, at: usize) -> usize {
    text[at..].chars().next().map_or(1, char::len_utf8)
}

fn instance_of(ty: Option<Ty>) -> Option<InstanceId> {
    match ty {
        Some(Ty::Instance { id, .. }) => id,
        _ => None,
    }
}

/// Every name `GetService` takes: the services in the game tree, and the
/// engine's other services (each API type whose description says it comes
/// from `game:GetService("Name")`).
pub(super) fn services(api: &Api, world: &World) -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = world
        .get(world.root())
        .map(|game| game.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| world.get(id))
        .map(|service| (service.class().to_string(), "service".to_owned()))
        .collect();
    names.extend(
        api.service_names
            .iter()
            .map(|name| (name.clone(), "service".to_owned())),
    );
    names
}

/// The attribute names `id` has, or (when it isn't known) every attribute
/// name in the world.
fn attributes(world: &World, id: Option<InstanceId>) -> Vec<(String, String)> {
    match id.and_then(|id| world.get(id)) {
        Some(instance) => instance
            .attributes()
            .keys()
            .map(|name| (name.clone(), "attribute".to_owned()))
            .collect(),
        None => {
            let mut found = Vec::new();
            for id in world.descendants(world.root()) {
                if let Some(instance) = world.get(id) {
                    found.extend(
                        instance
                            .attributes()
                            .keys()
                            .map(|name| (name.clone(), "attribute".to_owned())),
                    );
                }
            }
            found
        }
    }
}
