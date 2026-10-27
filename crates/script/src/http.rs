//! `game:GetService("HttpService")`, without the network: `JSONEncode`,
//! `JSONDecode` and `GenerateGUID`. Both sides have it.
//!
//! A table with keys 1, 2, 3, ... (or no keys) is a JSON array; a table
//! with text keys is a JSON object. JSON's `null` decodes to nil, so it
//! leaves a gap in a list. Numbers are Lua numbers, so whole numbers are
//! written without a decimal point.

use std::collections::BTreeSet;

use mlua::{Lua, MetaMethod, Table, UserData, UserDataMethods, Value};
use serde_json::{Map, Number, Value as Json};

use crate::random::LuaRandom;
use crate::types::type_name;

/// The HttpService, as scripts see it.
#[derive(Clone, Copy)]
pub struct LuaHttpService;

/// The largest whole number a Lua number holds exactly (2^53).
const EXACT_INTEGERS: f64 = 9_007_199_254_740_992.0;

impl UserData for LuaHttpService {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("JSONEncode", |lua, _, value: Value| {
            let json = to_json(lua, &value, &mut BTreeSet::new())
                .map_err(|message| mlua::Error::runtime(format!("JSONEncode {message}")))?;
            Ok(json.to_string())
        });
        methods.add_method("JSONDecode", |lua, _, text: Value| {
            let Value::String(text) = &text else {
                return Err(mlua::Error::runtime(format!(
                    "JSONDecode takes a string of JSON, not a {}",
                    type_name(lua, &text)
                )));
            };
            // serde_json's errors say what it expected and where:
            // "expected `,` or `}` at line 1 column 12".
            let json: Json = serde_json::from_slice(&text.as_bytes()[..]).map_err(|err| {
                mlua::Error::runtime(format!("JSONDecode can't read this JSON: {err}"))
            })?;
            from_json(lua, &json)
        });
        methods.add_method("GenerateGUID", |_, _, braces: Option<bool>| {
            let guid = generate_guid();
            Ok(if braces.unwrap_or(true) {
                format!("{{{guid}}}")
            } else {
                guid
            })
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("HttpService"));
    }
}

/// `value` as JSON. `tables` holds the tables being written around it, to
/// catch a table that contains itself. The error finishes "JSONEncode ...".
fn to_json(lua: &Lua, value: &Value, tables: &mut BTreeSet<usize>) -> Result<Json, String> {
    Ok(match value {
        Value::Nil => Json::Null,
        Value::Boolean(b) => Json::Bool(*b),
        Value::Integer(n) => Json::Number((*n).into()),
        Value::Number(n) => number(*n)?,
        Value::String(s) => match s.to_str() {
            Ok(text) => Json::String(text.to_owned()),
            Err(_) => return Err("can't write a string that isn't UTF-8 text".to_owned()),
        },
        Value::Table(table) => {
            // The table's address, only to recognize it again.
            let address = table.to_pointer() as usize;
            if !tables.insert(address) {
                return Err("can't write a table that contains itself".to_owned());
            }
            let json = table_to_json(lua, table, tables)?;
            tables.remove(&address);
            json
        }
        other => {
            return Err(format!(
                "can't write a {}; JSON holds numbers, strings, booleans, nil and tables",
                type_name(lua, other)
            ));
        }
    })
}

fn number(n: f64) -> Result<Json, String> {
    if !n.is_finite() {
        return Err(format!("can't write {n}: JSON has no infinity or NaN"));
    }
    if n.fract() == 0.0 && n.abs() < EXACT_INTEGERS {
        return Ok(Json::Number((n as i64).into()));
    }
    Ok(Number::from_f64(n).map_or(Json::Null, Json::Number))
}

/// A list (keys 1 to n, or empty) as an array, text keys as an object.
fn table_to_json(lua: &Lua, table: &Table, tables: &mut BTreeSet<usize>) -> Result<Json, String> {
    let mut pairs = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        pairs.push(pair.map_err(|err| err.to_string())?);
    }
    let length = table.raw_len();
    let is_list = pairs.len() == length
        && pairs.iter().all(|(key, _)| match key {
            Value::Integer(n) => (1..=length as i64).contains(n),
            Value::Number(n) => n.fract() == 0.0 && *n >= 1.0 && *n <= length as f64,
            _ => false,
        });
    if is_list {
        let mut items = Vec::with_capacity(length);
        for index in 1..=length {
            let item: Value = table.raw_get(index).map_err(|err| err.to_string())?;
            items.push(to_json(lua, &item, tables)?);
        }
        return Ok(Json::Array(items));
    }
    let mut object = Map::new();
    for (key, value) in &pairs {
        let Value::String(key) = key else {
            return Err(format!(
                "can't write a table with a {} key among others; JSON needs a list (keys 1, \
                 2, 3, ...) or text keys",
                type_name(lua, key)
            ));
        };
        let Ok(key) = key.to_str() else {
            return Err("can't write a key that isn't UTF-8 text".to_owned());
        };
        object.insert(key.to_owned(), to_json(lua, value, tables)?);
    }
    Ok(Json::Object(object))
}

/// JSON as a Lua value: arrays are lists from 1, objects tables by key.
fn from_json(lua: &Lua, json: &Json) -> mlua::Result<Value> {
    Ok(match json {
        Json::Null => Value::Nil,
        Json::Bool(b) => Value::Boolean(*b),
        Json::Number(n) => Value::Number(n.as_f64().unwrap_or(f64::NAN)),
        Json::String(s) => Value::String(lua.create_string(s)?),
        Json::Array(items) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for (index, item) in items.iter().enumerate() {
                table.raw_set(index + 1, from_json(lua, item)?)?;
            }
            Value::Table(table)
        }
        Json::Object(object) => {
            let table = lua.create_table_with_capacity(0, object.len())?;
            for (key, value) in object {
                table.raw_set(key.as_str(), from_json(lua, value)?)?;
            }
            Value::Table(table)
        }
    })
}

/// A random version 4 GUID, such as `"0F8FAD5B-D9CB-469F-A165-70867728950E"`.
/// Two generators with their own fresh seeds make its 122 random bits.
fn generate_guid() -> String {
    let high = LuaRandom::new(LuaRandom::fresh_seed()).next_u64();
    let low = LuaRandom::new(LuaRandom::fresh_seed()).next_u64();
    // The version (4) and the variant (binary 10) go in their fixed places.
    let high = (high & !0xF000) | 0x4000;
    let low = (low & !(0b11 << 62)) | (0b10 << 62);
    format!(
        "{:08X}-{:04X}-{:04X}-{:04X}-{:012X}",
        high >> 32,
        (high >> 16) & 0xFFFF,
        high & 0xFFFF,
        low >> 48,
        low & 0xFFFF_FFFF_FFFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guids_are_version_4_and_differ() {
        let a = generate_guid();
        let b = generate_guid();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4", "{a}");
        assert!("89AB".contains(&a[19..20]), "{a}");
    }
}
