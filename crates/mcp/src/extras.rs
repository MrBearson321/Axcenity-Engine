//! Prompts and resources: the two MCP features besides tools.
//!
//! A [`PromptSpec`] is a fixed recipe a client can offer its user (in
//! Claude Code, `/mcp__axcenity__axcenity_build`): a small model follows
//! steps instead of having to plan. A [`ResourceSpec`] mirrors a read-only
//! tool at a URI, for clients that attach resources to a conversation;
//! reading one runs the tool, so nothing is resource-only.

use serde_json::{Map, Value};

use crate::args::Args;
use crate::spec::ToolSpec;

/// One argument of a prompt: always a string.
#[derive(Debug)]
pub struct PromptArg {
    pub name: &'static str,
    pub about: &'static str,
    pub required: bool,
}

/// A prompt a client can list and fill in.
#[derive(Debug)]
pub struct PromptSpec {
    pub name: &'static str,
    pub title: &'static str,
    /// One sentence.
    pub description: &'static str,
    pub args: &'static [PromptArg],
    /// The message, from the arguments given (the required ones are there)
    /// and the tools this server lists, so a recipe only names those.
    pub render: fn(&Args, &[&'static ToolSpec]) -> String,
}

impl PromptSpec {
    /// The message for `arguments`, or what's missing.
    pub fn message(
        &self,
        arguments: Map<String, Value>,
        tools: &[&'static ToolSpec],
    ) -> Result<String, String> {
        for arg in self.args.iter().filter(|arg| arg.required) {
            if arguments
                .get(arg.name)
                .and_then(Value::as_str)
                .is_none_or(|text| text.trim().is_empty())
            {
                return Err(format!("{} needs {} ({})", self.name, arg.name, arg.about));
            }
        }
        Ok((self.render)(&Args::new(arguments), tools))
    }
}

/// A read-only tool at a URI. A `{name}` at the end makes it a template
/// whose value becomes the tool's parameter `name`:
/// `axcenity://object/{id}` reads as `object_get {"id": …}`.
#[derive(Debug)]
pub struct ResourceSpec {
    pub uri: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// The tool it mirrors.
    pub tool: &'static str,
}

impl ResourceSpec {
    /// The part before `{name}` and `name`, if this is a template.
    pub fn template(&self) -> Option<(&'static str, &'static str)> {
        let (prefix, rest) = self.uri.split_once('{')?;
        Some((prefix, rest.strip_suffix('}')?))
    }

    /// The tool's arguments when `uri` is this resource.
    pub fn args_for(&self, uri: &str) -> Option<Map<String, Value>> {
        match self.template() {
            None => (uri == self.uri).then(Map::new),
            Some((prefix, name)) => {
                let value = percent_decode(uri.strip_prefix(prefix)?)?;
                if value.is_empty() {
                    return None;
                }
                let mut args = Map::new();
                args.insert(name.to_owned(), Value::String(value));
                Some(args)
            }
        }
    }
}

/// `Workspace%2FHouse` → `Workspace/House`. `None` for a broken escape
/// or text that isn't UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBJECT: ResourceSpec = ResourceSpec {
        uri: "axcenity://object/{id}",
        name: "object",
        title: "Object",
        description: "One object's properties.",
        tool: "object_get",
    };

    #[test]
    fn a_template_turns_the_end_of_a_uri_into_the_parameter() {
        let args = OBJECT.args_for("axcenity://object/17").unwrap();
        assert_eq!(args["id"], "17");
        let args = OBJECT
            .args_for("axcenity://object/Workspace%2FHouse%20One")
            .unwrap();
        assert_eq!(args["id"], "Workspace/House One");
        assert!(OBJECT.args_for("axcenity://object/").is_none());
        assert!(OBJECT.args_for("axcenity://scene").is_none());
        assert!(OBJECT.args_for("axcenity://object/%zz").is_none());
    }

    #[test]
    fn a_plain_resource_matches_only_its_own_uri() {
        let scene = ResourceSpec {
            uri: "axcenity://scene",
            ..OBJECT
        };
        assert!(scene.template().is_none());
        assert!(scene.args_for("axcenity://scene").unwrap().is_empty());
        assert!(scene.args_for("axcenity://scene/x").is_none());
    }
}
