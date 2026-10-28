//! Placing prefabs and keeping their copies up to date (Phase 49).
//!
//! World and prefab files write a placed copy in full, so they read on
//! their own, with its link and overrides marked (`docs/project/world.md`).
//! [`Prefabs`] reads prefab files and makes each copy match its prefab as
//! the file is now, keeping the copy's overrides ([`Prefabs::refresh`]).
//! A prefab holding copies of other prefabs (nested prefabs, and variants,
//! whose top-level Instance is a copy of their base) is expanded the same
//! way first. A prefab that can't be read leaves its copies as they were
//! saved, and is reported.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axcenity_core::prefab::{self, PrefabLink, PrefabSource};
use axcenity_core::{InstanceId, World};

use crate::world_file::parse_prefab;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PrefabError {
    #[error("the prefab `{path}` could not be read: {message}")]
    Read { path: String, message: String },
    #[error("`{path}` is not a valid prefab: {message}")]
    Invalid { path: String, message: String },
    #[error("the prefab `{path}` contains a copy of itself")]
    Cycle { path: String },
}

impl PrefabError {
    /// The prefab file it's about.
    pub fn path(&self) -> &str {
        match self {
            Self::Read { path, .. } | Self::Invalid { path, .. } | Self::Cycle { path } => path,
        }
    }
}

type Reader<'a> = Box<dyn FnMut(&str) -> Result<String, String> + 'a>;

/// Reads prefab files (once each) and expands them.
pub struct Prefabs<'a> {
    read: Reader<'a>,
    sources: HashMap<String, Result<Arc<PrefabSource>, PrefabError>>,
    /// The prefabs being expanded, to catch one that holds itself.
    loading: Vec<String>,
    problems: Vec<PrefabError>,
}

impl<'a> Prefabs<'a> {
    /// Reads prefab files with `read`, given a path from the project folder.
    pub fn new(read: impl FnMut(&str) -> Result<String, String> + 'a) -> Self {
        Self {
            read: Box::new(read),
            sources: HashMap::new(),
            loading: Vec::new(),
            problems: Vec::new(),
        }
    }

    /// Reads prefab files from the project in `folder`.
    pub fn in_folder(folder: &'a Path) -> Self {
        Self::new(move |path| {
            std::fs::read_to_string(folder.join(path)).map_err(|err| err.to_string())
        })
    }

    /// What the prefab at `path` holds, with the copies inside it up to date.
    pub fn source(&mut self, path: &str) -> Result<Arc<PrefabSource>, PrefabError> {
        if let Some(found) = self.sources.get(path) {
            return found.clone();
        }
        if self.loading.iter().any(|loading| loading == path) {
            return Err(PrefabError::Cycle {
                path: path.to_owned(),
            });
        }
        self.loading.push(path.to_owned());
        let result = self.expand(path);
        self.loading.pop();
        self.sources.insert(path.to_owned(), result.clone());
        result
    }

    fn expand(&mut self, path: &str) -> Result<Arc<PrefabSource>, PrefabError> {
        let text = (self.read)(path).map_err(|message| PrefabError::Read {
            path: path.to_owned(),
            message,
        })?;
        let mut world = World::new();
        let workspace = world.workspace();
        let roots =
            parse_prefab(&text, &mut world, workspace).map_err(|err| PrefabError::Invalid {
                path: path.to_owned(),
                message: err.to_string(),
            })?;
        for &root in &roots {
            self.refresh(&mut world, root);
        }
        Ok(Arc::new(PrefabSource { world, roots }))
    }

    /// Adds a linked copy of the prefab at `path` inside `parent` and
    /// returns its top-level Instances.
    pub fn instantiate(
        &mut self,
        path: &str,
        world: &mut World,
        parent: InstanceId,
    ) -> Result<Vec<InstanceId>, PrefabError> {
        let source = self.source(path)?;
        let mut added = Vec::new();
        for (index, &root) in source.roots.iter().enumerate() {
            let Ok(copy) = world.copy_from(&source.world, root) else {
                continue;
            };
            let _ = world.set_parent(copy, Some(parent));
            let mut link = PrefabLink::new(path, index);
            link.source = Some(source.clone());
            let _ = world.set_prefab(copy, Some(link));
            added.push(copy);
        }
        Ok(added)
    }

    /// Makes every copy in `id` (and `id` itself) match its prefab as the
    /// file is now, keeping its overrides. A copy whose prefab can't be
    /// read stays as it is, and the problem is kept for
    /// [`take_problems`](Self::take_problems).
    pub fn refresh(&mut self, world: &mut World, id: InstanceId) {
        let Some(instance) = world.get(id) else {
            return;
        };
        let Some(link) = instance.prefab().cloned() else {
            let children = instance.children().to_vec();
            for child in children {
                self.refresh(world, child);
            }
            return;
        };
        let overrides = prefab::overrides(world, id);
        match self.source(&link.path) {
            Ok(source) => prefab::sync(world, id, &source, link.index, &overrides),
            Err(problem) => {
                if !self.problems.contains(&problem) {
                    self.problems.push(problem);
                }
            }
        }
        // What was added to the copy may hold copies of its own.
        for added in prefab::added_children(world, id) {
            self.refresh(world, added);
        }
    }

    /// The prefabs that couldn't be read since the last call, each once.
    pub fn take_problems(&mut self) -> Vec<PrefabError> {
        std::mem::take(&mut self.problems)
    }
}

/// Every prefab file a world or prefab file's text names (the prefabs its
/// copies are of), each once, in the order they first appear. Nothing for
/// text that isn't JSON.
pub fn prefab_references(text: &str) -> Vec<String> {
    fn walk(json: &serde_json::Value, out: &mut Vec<String>) {
        match json {
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::String(path)) = map.get("prefab")
                    && !out.contains(path)
                {
                    out.push(path.clone());
                }
                for (key, value) in map {
                    // Property values can't hold copies.
                    if key != "properties" && key != "attributes" {
                        walk(value, out);
                    }
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
        walk(&json, &mut out);
    }
    out
}

/// The text of a new variant of the prefab at `base`: a prefab whose
/// top-level Instances are copies of `base`'s, with no overrides yet.
pub fn variant_text(prefabs: &mut Prefabs, base: &str) -> Result<String, PrefabError> {
    let mut world = World::new();
    let workspace = world.workspace();
    let roots = prefabs.instantiate(base, &mut world, workspace)?;
    Ok(crate::world_file::write_prefab(&world, &roots))
}

/// The text of the prefab at `path` after Apply overrides: the copy at
/// `root` in `world` becomes the prefab's top-level Instance it is a copy
/// of. Keeps a variant's base and the prefab's other top-level Instances.
pub fn applied_text(
    prefabs: &mut Prefabs,
    world: &World,
    root: InstanceId,
) -> Result<String, PrefabError> {
    let link = world
        .get(root)
        .and_then(|instance| instance.prefab())
        .cloned()
        .ok_or_else(|| PrefabError::Read {
            path: String::new(),
            message: "it isn't a copy of a prefab".to_owned(),
        })?;
    let source = match link.source.clone() {
        Some(source) => source,
        None => prefabs.source(&link.path)?,
    };
    let mut result = (*source).clone();
    let Some(&old) = result.roots.get(link.index) else {
        return Err(PrefabError::Invalid {
            path: link.path,
            message: "the prefab has fewer Instances than when this copy was placed".to_owned(),
        });
    };
    let base = result
        .world
        .get(old)
        .and_then(|instance| instance.prefab())
        .cloned();
    let workspace = result.world.workspace();
    let index_in_parent = result
        .world
        .get(workspace)
        .and_then(|w| w.children().iter().position(|&c| c == old))
        .unwrap_or(0);
    let new = result
        .world
        .copy_from(world, root)
        .map_err(|err| PrefabError::Invalid {
            path: link.path.clone(),
            message: err.to_string(),
        })?;
    let _ = result.world.set_prefab(new, base);
    let _ = result.world.destroy(old);
    let _ = result.world.set_parent_at(new, workspace, index_in_parent);
    result.roots[link.index] = new;
    Ok(crate::world_file::write_prefab(
        &result.world,
        &result.roots,
    ))
}

#[cfg(test)]
mod tests;
