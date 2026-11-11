//! A call's arguments, read with errors a model can fix: the missing or
//! wrong parameter is named, with what it should look like.

use serde_json::{Map, Value};

use crate::error::ToolError;
use crate::spec::{Kind, ToolSpec};

/// How much a reading tool returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Detail {
    Min,
    Std,
    Full,
}

impl Detail {
    pub const NAMES: &'static [&'static str] = &["min", "std", "full"];
}

/// The arguments of one call.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args(pub Map<String, Value>);

impl Args {
    pub fn new(map: Map<String, Value>) -> Args {
        Args(map)
    }

    /// Checks the arguments against the tool's parameters: no unknown
    /// names, every required one present, each of the right type. Clients
    /// don't all validate, and a model reads this message better than a
    /// schema error.
    pub fn check(&self, spec: &ToolSpec) -> Result<(), ToolError> {
        for (name, value) in &self.0 {
            let Some(param) = spec.param(name) else {
                let names: Vec<&str> = spec.params.iter().map(|p| p.name).collect();
                let mut error =
                    ToolError::bad_args(format!("{} has no parameter \"{name}\"", spec.name));
                error = error.with_fix(if names.is_empty() {
                    "it takes no parameters".to_owned()
                } else {
                    format!("its parameters are {}", names.join(", "))
                });
                return Err(error);
            };
            if value.is_null() {
                continue;
            }
            if !fits(param.kind, value) {
                return Err(ToolError::bad_args(format!(
                    "{name} should be {}, not {}",
                    describe(param.kind),
                    short(value)
                )));
            }
        }
        for param in spec.params.iter().filter(|p| p.required) {
            if self.0.get(param.name).is_none_or(Value::is_null) {
                return Err(ToolError::bad_args(format!(
                    "{} needs {} ({})",
                    spec.name,
                    param.name,
                    describe(param.kind)
                )));
            }
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.get(name).filter(|value| !value.is_null())
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(Value::as_str)
    }

    pub fn u64(&self, name: &str) -> Option<u64> {
        self.get(name).and_then(Value::as_u64)
    }

    pub fn f64(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(Value::as_f64)
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        self.get(name).and_then(Value::as_bool)
    }

    pub fn strings(&self, name: &str) -> Option<Vec<&str>> {
        self.get(name)
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect())
    }

    /// A [`Kind::Pairs`] argument's pairs, if it's there.
    pub fn pairs(&self, name: &str, first: &str, second: &str) -> Option<Vec<(&str, &str)>> {
        let items = self.get(name)?.as_array()?;
        items
            .iter()
            .map(|item| Some((item.get(first)?.as_str()?, item.get(second)?.as_str()?)))
            .collect()
    }

    pub fn vector3(&self, name: &str) -> Option<[f64; 3]> {
        let items = self.get(name)?.as_array()?;
        Some([items[0].as_f64()?, items[1].as_f64()?, items[2].as_f64()?])
    }

    /// `detail`, or `default` when it's left out.
    pub fn detail(&self, default: Detail) -> Detail {
        match self.str("detail") {
            Some("min") => Detail::Min,
            Some("std") => Detail::Std,
            Some("full") => Detail::Full,
            _ => default,
        }
    }

    /// `limit`, defaulting to `default` and never above `cap`.
    pub fn limit(&self, default: usize, cap: usize) -> usize {
        self.u64("limit")
            .map_or(default, |n| usize::try_from(n).unwrap_or(cap))
            .clamp(1, cap)
    }
}

fn fits(kind: Kind, value: &Value) -> bool {
    match kind {
        Kind::String => value.is_string(),
        Kind::Integer => value.is_u64() || value.is_i64(),
        Kind::Number => value.is_number(),
        Kind::Bool => value.is_boolean(),
        Kind::Enum(choices) => value.as_str().is_some_and(|s| choices.contains(&s)),
        Kind::Strings => value
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string)),
        Kind::Vector3 => value
            .as_array()
            .is_some_and(|items| items.len() == 3 && items.iter().all(Value::is_number)),
        Kind::Object => value.is_object(),
        Kind::Pairs(first, second) => value.as_array().is_some_and(|items| {
            items.iter().all(|item| {
                item.get(first).is_some_and(Value::is_string)
                    && item.get(second).is_some_and(Value::is_string)
            })
        }),
    }
}

fn describe(kind: Kind) -> String {
    match kind {
        Kind::String => "a string".into(),
        Kind::Integer => "a whole number".into(),
        Kind::Number => "a number".into(),
        Kind::Bool => "true or false".into(),
        Kind::Enum(choices) => format!("one of {}", choices.join(", ")),
        Kind::Strings => "a list of strings, like [\"17\",\"Workspace/Door\"]".into(),
        Kind::Vector3 => "three numbers, like [0,5,0]".into(),
        Kind::Object => "an object, like {\"Anchored\":true}".into(),
        Kind::Pairs(first, second) => {
            format!("a list like [{{\"{first}\":\"…\",\"{second}\":\"…\"}}]")
        }
    }
}

/// A value shortened for an error message.
fn short(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 40 {
        let cut: String = text.chars().take(37).collect();
        format!("{cut}...")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::spec::{Class, Param, Profile};

    const SPEC: ToolSpec = ToolSpec {
        name: "object_get",
        title: "Get objects",
        summary: "Properties of objects.",
        profile: Profile::Lite,
        class: Class::Read,
        idempotent: true,
        params: &[
            Param::required("id", Kind::String, "object"),
            Param::optional("detail", Kind::Enum(Detail::NAMES), ""),
            Param::optional("near", Kind::Vector3, ""),
            Param::optional("limit", Kind::Integer, ""),
        ],
    };

    fn args(value: Value) -> Args {
        Args::new(value.as_object().unwrap().clone())
    }

    #[test]
    fn checking_names_the_parameter_and_what_it_should_be() {
        assert_eq!(args(json!({"id": "17"})).check(&SPEC), Ok(()));
        // Null counts as left out.
        assert_eq!(
            args(json!({"id": "17", "detail": null})).check(&SPEC),
            Ok(())
        );
        assert_eq!(
            args(json!({})).check(&SPEC).unwrap_err().to_string(),
            "error bad_args: object_get needs id (a string)"
        );
        assert_eq!(
            args(json!({"id": "17", "detail": "max"}))
                .check(&SPEC)
                .unwrap_err()
                .to_string(),
            "error bad_args: detail should be one of min, std, full, not \"max\""
        );
        assert_eq!(
            args(json!({"id": "17", "near": [1, 2]}))
                .check(&SPEC)
                .unwrap_err()
                .to_string(),
            "error bad_args: near should be three numbers, like [0,5,0], not [1,2]"
        );
        assert_eq!(
            args(json!({"id": "17", "colour": "red"}))
                .check(&SPEC)
                .unwrap_err()
                .to_string(),
            "error bad_args: object_get has no parameter \"colour\"\nfix: its parameters are id, detail, near, limit"
        );
    }

    #[test]
    fn readers_give_defaults_and_caps() {
        let a = args(json!({"id": "17", "detail": "full", "near": [1, 2.5, -3], "limit": 9999}));
        assert_eq!(a.detail(Detail::Std), Detail::Full);
        assert_eq!(args(json!({})).detail(Detail::Min), Detail::Min);
        assert_eq!(a.vector3("near"), Some([1.0, 2.5, -3.0]));
        assert_eq!(a.limit(50, 500), 500);
        assert_eq!(args(json!({"limit": 0})).limit(50, 500), 1);
        assert_eq!(args(json!({})).limit(50, 500), 50);
    }
}

#[cfg(test)]
mod pairs_tests {
    use serde_json::json;

    use super::*;
    use crate::{Class, Param, Profile, ToolSpec};

    const EDITS: ToolSpec = ToolSpec {
        name: "edit",
        title: "Edit",
        summary: "Edits.",
        profile: Profile::Lite,
        class: Class::Modify,
        idempotent: false,
        params: &[Param::optional("edits", Kind::Pairs("find", "replace"), "")],
    };

    #[test]
    fn pairs_are_lists_of_two_named_strings() {
        let good = Args::new(
            json!({"edits": [{"find": "a", "replace": "b"}]})
                .as_object()
                .cloned()
                .unwrap(),
        );
        assert_eq!(good.check(&EDITS), Ok(()));
        assert_eq!(
            good.pairs("edits", "find", "replace"),
            Some(vec![("a", "b")])
        );
        let bad = Args::new(
            json!({"edits": [{"find": "a"}]})
                .as_object()
                .cloned()
                .unwrap(),
        );
        let error = bad.check(&EDITS).unwrap_err();
        assert!(
            error
                .message
                .contains("[{\"find\":\"…\",\"replace\":\"…\"}]"),
            "{error}"
        );
        let schema = serde_json::Value::Object(EDITS.input_schema()).to_string();
        assert!(
            schema.contains(r#""required":["find","replace"]"#),
            "{schema}"
        );
    }
}
