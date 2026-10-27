//! `game:GetService("LocalizationService")`: the language UI text shows,
//! from the project's string table (`strings.json`; see
//! [`axcenity_core::StringTable`]).
//!
//! `Language` is the active language. Setting it changes every TextLabel
//! and TextButton whose `TextKey` names an entry, the next time the UI is
//! drawn. The table and the language belong to the world, so both sides
//! see the same ones.

use mlua::{MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::instance::{world, world_mut};
use crate::types::type_name;

/// `LocalizationService`, as scripts see it. Holds nothing itself.
#[derive(Clone, Copy)]
pub struct LuaLocalizationService;

impl UserData for LuaLocalizationService {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Language", |lua, _| Ok(world(lua)?.language().to_owned()));
        fields.add_field_method_set("Language", |lua, _, value: mlua::Value| {
            let mlua::Value::String(language) = &value else {
                return Err(mlua::Error::runtime(format!(
                    "LocalizationService.Language must be the name of a language in the string table, \
                     such as \"English\", not a {}",
                    type_name(lua, &value)
                )));
            };
            let language = language.to_string_lossy();
            let mut world = world_mut(lua)?;
            if world.set_language(&language) {
                return Ok(());
            }
            let languages = &world.strings().languages;
            Err(mlua::Error::runtime(if languages.is_empty() {
                format!(
                    "the string table has no languages, so \"{language}\" can't be the active one; \
                     add languages in the UI Designer's Strings tab"
                )
            } else {
                format!(
                    "the string table has no language \"{language}\"; choose from {}",
                    languages.join(", ")
                )
            }))
        });
        fields.add_field_method_get("Languages", |lua, _| {
            lua.create_sequence_from(world(lua)?.strings().languages.clone())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetString", |lua, _, key: mlua::Value| {
            let mlua::Value::String(key) = &key else {
                return Err(mlua::Error::runtime(format!(
                    "GetString needs a key from the string table, such as \
                     LocalizationService:GetString(\"menu.play\"), not a {}",
                    type_name(lua, &key)
                )));
            };
            let key = key.to_string_lossy();
            Ok(world(lua)?.translate(&key).map(str::to_owned))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("LocalizationService"));
    }
}
