//! The project's string table: UI text in more than one language.
//!
//! Each entry has a key (such as `menu.play`) and a value per language. A
//! TextLabel or TextButton whose `TextKey` names an entry shows that entry's
//! value in the world's active language ([`World::language`](crate::World::language)),
//! or its own `Text` (the source text) when the entry or that language's
//! value is missing. The `project` crate reads and writes it as `strings.json`.

use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable {
    /// The languages, in the order the editor shows them. The first is the
    /// one a game starts in.
    pub languages: Vec<String>,
    /// Each key's value per language. A language with no value is left out.
    pub entries: BTreeMap<String, BTreeMap<String, String>>,
}

impl StringTable {
    /// `key`'s value in `language`, if it has one that isn't empty.
    pub fn get(&self, key: &str, language: &str) -> Option<&str> {
        self.entries
            .get(key)?
            .get(language)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    /// The language a game starts in: the first, or none.
    pub fn default_language(&self) -> &str {
        self.languages.first().map_or("", String::as_str)
    }

    pub fn has_language(&self, language: &str) -> bool {
        self.languages.iter().any(|known| known == language)
    }

    pub fn is_empty(&self) -> bool {
        self.languages.is_empty() && self.entries.is_empty()
    }

    /// Sets `key`'s value in `language`. An empty value removes it, but the
    /// key stays.
    pub fn set(&mut self, key: &str, language: &str, value: &str) {
        let values = self.entries.entry(key.to_owned()).or_default();
        if value.is_empty() {
            values.remove(language);
        } else {
            values.insert(language.to_owned(), value.to_owned());
        }
    }

    /// Renames the key `from` to `to`, keeping its values. False (and
    /// nothing changes) if `from` isn't there, or `to` is empty or taken.
    pub fn rename_key(&mut self, from: &str, to: &str) -> bool {
        if to.is_empty() || self.entries.contains_key(to) {
            return false;
        }
        let Some(values) = self.entries.remove(from) else {
            return false;
        };
        self.entries.insert(to.to_owned(), values);
        true
    }

    /// Adds `language` at the end. False if it's empty or already there.
    pub fn add_language(&mut self, language: &str) -> bool {
        if language.is_empty() || self.has_language(language) {
            return false;
        }
        self.languages.push(language.to_owned());
        true
    }

    /// Removes `language` and every value in it.
    pub fn remove_language(&mut self, language: &str) {
        self.languages.retain(|known| known != language);
        for values in self.entries.values_mut() {
            values.remove(language);
        }
    }
}
