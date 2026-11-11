//! What a tool is: its name, one-sentence summary, safety class, which
//! profiles list it, and its parameters. A [`ToolSpec`] is plain data, so the
//! editor's tool table is a `const` array and the JSON Schema clients see is
//! built from it in one place ([`ToolSpec::input_schema`]).

use serde_json::{Map, Value, json};

/// How much a call can change, from the safest up. A tool has a default
/// class; a call can be classed higher by what it would do (a delete of 3
/// objects is [`Class::Modify`], of 300 is [`Class::Destructive`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Only reads.
    Read,
    /// Creates something; undo removes it.
    Add,
    /// Changes something; undo puts it back.
    Modify,
    /// Removes or overwrites a lot, or something hard to see: needs a preview
    /// and a confirm token.
    Destructive,
}

impl Class {
    /// Its name in logs: `read`, `add`, `modify` or `destructive`.
    pub fn name(self) -> &'static str {
        match self {
            Class::Read => "read",
            Class::Add => "add",
            Class::Modify => "modify",
            Class::Destructive => "destructive",
        }
    }
}

/// Which tool lists a server shows. A profile only hides tools: every tool
/// behaves the same in every profile that has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Profile {
    /// For small local models: the fewest tools.
    Lite,
    /// The default.
    Standard,
    /// Everything.
    Full,
}

impl Profile {
    pub const ALL: [Profile; 3] = [Profile::Lite, Profile::Standard, Profile::Full];

    pub fn from_name(name: &str) -> Option<Profile> {
        match name {
            "lite" => Some(Profile::Lite),
            "standard" => Some(Profile::Standard),
            "full" => Some(Profile::Full),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Profile::Lite => "lite",
            Profile::Standard => "standard",
            Profile::Full => "full",
        }
    }

    /// Whether a tool first listed in `first` is in this profile. Profiles
    /// nest: lite's tools are in standard, and standard's in full.
    pub fn shows(self, first: Profile) -> bool {
        first <= self
    }
}

/// The JSON type of a parameter, kept to what every client and model API
/// accepts: no `oneOf`, no `$ref`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    String,
    Integer,
    Number,
    Bool,
    /// One of these strings.
    Enum(&'static [&'static str]),
    /// A list of strings.
    Strings,
    /// `[x, y, z]`.
    Vector3,
    /// A flat object, such as `props`.
    Object,
    /// A list of objects with these two string fields, such as
    /// `[{"find":…,"replace":…}]`.
    Pairs(&'static str, &'static str),
}

/// One parameter. Tools share the common ones as constants (`ID`, `DETAIL`,
/// …), so a name means the same thing, with the same words, everywhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    /// A few words; the schema is what models read, so keep it short.
    pub about: &'static str,
    pub required: bool,
}

impl Param {
    pub const fn optional(name: &'static str, kind: Kind, about: &'static str) -> Param {
        Param {
            name,
            kind,
            about,
            required: false,
        }
    }

    pub const fn required(name: &'static str, kind: Kind, about: &'static str) -> Param {
        Param {
            name,
            kind,
            about,
            required: true,
        }
    }

    fn schema(&self) -> Value {
        let mut schema = match self.kind {
            Kind::String => json!({ "type": "string" }),
            Kind::Integer => json!({ "type": "integer" }),
            Kind::Number => json!({ "type": "number" }),
            Kind::Bool => json!({ "type": "boolean" }),
            Kind::Enum(values) => json!({ "type": "string", "enum": values }),
            Kind::Strings => json!({ "type": "array", "items": { "type": "string" } }),
            Kind::Vector3 => json!({
                "type": "array",
                "items": { "type": "number" },
                "minItems": 3,
                "maxItems": 3
            }),
            Kind::Object => json!({ "type": "object" }),
            Kind::Pairs(first, second) => json!({
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { first: { "type": "string" }, second: { "type": "string" } },
                    "required": [first, second]
                }
            }),
        };
        if !self.about.is_empty() {
            schema["description"] = Value::String(self.about.to_owned());
        }
        schema
    }
}

/// A tool as clients see it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolSpec {
    /// `noun_verb`, snake case, at most 20 characters.
    pub name: &'static str,
    /// Shown by clients that show titles.
    pub title: &'static str,
    /// One sentence. Value formats and lists live in the `docs` tool.
    pub summary: &'static str,
    /// The first (smallest) profile that lists it.
    pub profile: Profile,
    /// Its class when nothing about the call raises it.
    pub class: Class,
    /// Calls with the same arguments give the same result (for clients
    /// that retry).
    pub idempotent: bool,
    pub params: &'static [Param],
}

impl ToolSpec {
    /// The JSON Schema of its arguments: a plain object of its parameters,
    /// with `required` listing the required ones.
    pub fn input_schema(&self) -> Map<String, Value> {
        let mut properties = Map::new();
        for param in self.params {
            properties.insert(param.name.to_owned(), param.schema());
        }
        let mut schema = Map::new();
        schema.insert("type".into(), "object".into());
        schema.insert("properties".into(), Value::Object(properties));
        let required: Vec<Value> = self
            .params
            .iter()
            .filter(|param| param.required)
            .map(|param| param.name.into())
            .collect();
        if !required.is_empty() {
            schema.insert("required".into(), Value::Array(required));
        }
        schema
    }

    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|param| param.name == name)
    }
}
