//! Reading sources' JSON answers without trusting their shape: a missing
//! or wrongly typed field is empty, never a panic.

use serde_json::Value;

use crate::SourceError;

/// The largest JSON answer read from a source (Poly Haven's full texture
/// catalogue is a few megabytes).
pub(crate) const JSON_LIMIT: u64 = 32 * 1024 * 1024;

/// Parses a source's answer; anything that isn't JSON is a bad response.
pub(crate) fn parse(text: &str) -> Result<Value, SourceError> {
    serde_json::from_str(text)
        .map_err(|_| SourceError::BadResponse("the answer was not understood"))
}

/// A string field, trimmed; `""` when missing or not a string.
pub(crate) fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_owned()
}

/// A whole number of bytes or a count; `None` when missing or negative.
pub(crate) fn count(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

/// A list of names: strings, or objects with a `name` (Openverse's tags).
/// Blank entries are dropped.
pub(crate) fn names(value: Option<&Value>) -> Vec<String> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| match item {
            Value::Object(object) => text(object.get("name")),
            other => text(Some(other)),
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// A file URL's file name: its `file=` query value (ambientCG's downloads),
/// else the last part of its path when that has an extension, else
/// `fallback`.
pub(crate) fn file_name(url: &str, fallback: &str) -> String {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let named = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("file="))
        .filter(|name| !name.is_empty());
    let last = path.rsplit('/').next().unwrap_or_default();
    let candidate = named.unwrap_or(if last.trim_matches('.').contains('.') {
        last
    } else {
        ""
    });
    let candidate = candidate.replace(['/', '\\'], "");
    if candidate.is_empty() || candidate.starts_with('.') {
        fallback.to_owned()
    } else {
        candidate
    }
}

/// Lowercase words of letters and digits, for matching searches.
pub(crate) fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn fields_of_the_wrong_type_read_as_empty() {
        let value = json!({"name": 5, "size": -1, "tags": ["a", "", {"name": "b"}, 3]});
        assert_eq!(text(value.get("name")), "");
        assert_eq!(count(value.get("size")), None);
        assert_eq!(names(value.get("tags")), ["a", "b"]);
        assert!(names(value.get("missing")).is_empty());
    }

    #[test]
    fn file_names_come_from_the_query_or_the_path() {
        assert_eq!(
            file_name("https://ambientcg.com/get?file=Rock_2K-JPG.zip", "x"),
            "Rock_2K-JPG.zip"
        );
        assert_eq!(
            file_name("https://dl.polyhaven.org/a/b/brick_diff_2k.jpg", "x"),
            "brick_diff_2k.jpg"
        );
        assert_eq!(
            file_name("https://freesound.org/get", "door.mp3"),
            "door.mp3"
        );
        assert_eq!(
            file_name("https://x.org/get?file=../evil.png", "safe"),
            "safe"
        );
        assert_eq!(file_name("https://x.org/get?file=.hidden", "safe"), "safe");
    }
}
