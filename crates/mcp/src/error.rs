//! Tool errors: one shape for every tool, written for a model to act on.
//! They're tool results with `isError: true`, never protocol errors, so the
//! model reads them (the spec says input validation failures go this way).
//!
//! ```text
//! error not_found: no object "Workspace/Hous"
//! fix: did you mean "Workspace/House"? scene_inspect lists objects.
//! ```

use std::fmt;

/// What went wrong, as a short stable word the model can branch on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    /// A parameter is missing, of the wrong type, or not one of the choices.
    BadArgs,
    /// No such object, file, class or topic.
    NotFound,
    /// The ID named an object that was deleted.
    StaleId,
    /// A path or name matched more than one object.
    Ambiguous,
    /// The class has no such property.
    UnknownProperty,
    /// The value has the wrong type or is out of range.
    BadValue,
    /// The object is Locked.
    Locked,
    /// The permission mode doesn't allow it.
    NotAllowed,
    /// A destructive call needs `confirm`.
    NeedsConfirm,
    /// Over a size limit.
    TooLarge,
    /// The editor is busy (Play, a drag, a job).
    Busy,
    /// The engine can't do this.
    Unsupported,
    /// A bug or an unexpected engine failure.
    Internal,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Code::BadArgs => "bad_args",
            Code::NotFound => "not_found",
            Code::StaleId => "stale_id",
            Code::Ambiguous => "ambiguous",
            Code::UnknownProperty => "unknown_property",
            Code::BadValue => "bad_value",
            Code::Locked => "locked",
            Code::NotAllowed => "not_allowed",
            Code::NeedsConfirm => "needs_confirm",
            Code::TooLarge => "too_large",
            Code::Busy => "busy",
            Code::Unsupported => "unsupported",
            Code::Internal => "internal",
        }
    }
}

/// A failed call: what happened, and what to do about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError {
    pub code: Code,
    pub message: String,
    /// What to do next, such as a corrected value or a tool to call.
    pub fix: Option<String>,
}

impl ToolError {
    pub fn new(code: Code, message: impl Into<String>) -> ToolError {
        ToolError {
            code,
            message: message.into(),
            fix: None,
        }
    }

    pub fn with_fix(mut self, fix: impl Into<String>) -> ToolError {
        self.fix = Some(fix.into());
        self
    }

    pub fn bad_args(message: impl Into<String>) -> ToolError {
        ToolError::new(Code::BadArgs, message)
    }

    pub fn not_found(message: impl Into<String>) -> ToolError {
        ToolError::new(Code::NotFound, message)
    }

    pub fn internal(message: impl Into<String>) -> ToolError {
        ToolError::new(Code::Internal, message)
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error {}: {}", self.code.as_str(), self.message)?;
        if let Some(fix) = &self.fix {
            write!(f, "\nfix: {fix}")?;
        }
        Ok(())
    }
}

/// Up to `count` of `choices` closest to `wanted` (case-insensitive edit
/// distance, then prefix and substring matches first), for "did you mean"
/// fixes. Only choices within a third of the word's length (at least 2
/// edits) away, or containing it, count as close.
pub fn closest<'a>(
    wanted: &str,
    choices: impl IntoIterator<Item = &'a str>,
    count: usize,
) -> Vec<&'a str> {
    let wanted_lower = wanted.to_lowercase();
    let limit = (wanted.chars().count() / 3).max(2);
    let mut scored: Vec<(usize, &str)> = choices
        .into_iter()
        .filter_map(|choice| {
            let lower = choice.to_lowercase();
            let distance = edit_distance(&wanted_lower, &lower);
            let contains = !wanted_lower.is_empty()
                && (lower.contains(&wanted_lower) || wanted_lower.contains(&lower));
            if distance <= limit {
                Some((distance, choice))
            } else if contains {
                Some((limit + 1, choice))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));
    scored.dedup_by(|a, b| a.1 == b.1);
    scored
        .into_iter()
        .take(count)
        .map(|(_, choice)| choice)
        .collect()
}

/// Levenshtein distance over characters.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1; b.len() + 1];
        for (j, &cb) in b.iter().enumerate() {
            let substitute = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitute.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_reads_as_its_code_message_and_fix() {
        let error = ToolError::not_found("no object \"Workspace/Hous\"")
            .with_fix("did you mean \"Workspace/House\"?");
        assert_eq!(
            error.to_string(),
            "error not_found: no object \"Workspace/Hous\"\nfix: did you mean \"Workspace/House\"?"
        );
        assert_eq!(
            ToolError::bad_args("id is required").to_string(),
            "error bad_args: id is required"
        );
    }

    #[test]
    fn closest_finds_typos_and_case_and_ignores_the_unrelated() {
        let props = ["Color", "Size", "Position", "Transparency", "Material"];
        assert_eq!(closest("Colour", props, 3), ["Color"]);
        assert_eq!(closest("position", props, 3), ["Position"]);
        assert_eq!(closest("Transp", props, 3), ["Transparency"]);
        assert!(closest("Gravity", props, 3).is_empty());
    }
}
