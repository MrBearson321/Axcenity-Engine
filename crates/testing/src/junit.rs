//! Test results as JUnit XML, which CI services read (`axcenity-test
//! --junit`): one `testsuite` per spec file, one `testcase` per test.

use std::fmt::Write;

use crate::TestResult;

/// `results` (in run order) as a JUnit XML document, for the project `name`.
pub fn junit_xml(name: &str, results: &[TestResult]) -> String {
    let failures = results.iter().filter(|result| !result.passed()).count();
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        xml,
        "<testsuites name=\"{}\" tests=\"{}\" failures=\"{failures}\">",
        escape(name),
        results.len()
    );
    let mut files: Vec<&str> = Vec::new();
    for result in results {
        if !files.contains(&result.file.as_str()) {
            files.push(&result.file);
        }
    }
    for file in files {
        let cases: Vec<&TestResult> = results.iter().filter(|r| r.file == file).collect();
        let failed = cases.iter().filter(|result| !result.passed()).count();
        let time: f64 = cases.iter().map(|result| result.seconds).sum();
        let _ = writeln!(
            xml,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{failed}\" time=\"{time:.3}\">",
            escape(file),
            cases.len()
        );
        for case in cases {
            let _ = write!(
                xml,
                "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"",
                escape(file),
                escape(&case.title()),
                case.seconds
            );
            if case.passed() && case.output.is_empty() {
                xml.push_str("/>\n");
                continue;
            }
            xml.push_str(">\n");
            if let Some(failure) = &case.failure {
                let first = failure.message.lines().next().unwrap_or_default();
                let _ = writeln!(
                    xml,
                    "      <failure message=\"{}\">{}</failure>",
                    escape(first),
                    escape(&failure.message)
                );
            }
            if !case.output.is_empty() {
                let _ = writeln!(
                    xml,
                    "      <system-out>{}</system-out>",
                    escape(&case.output.join("\n"))
                );
            }
            xml.push_str("    </testcase>\n");
        }
        xml.push_str("  </testsuite>\n");
    }
    xml.push_str("</testsuites>\n");
    xml
}

/// `text` safe inside XML text and attribute values. Control characters
/// XML can't hold at all are left out.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\n' | '\t' | '\r' => escaped.push(c),
            c if c < ' ' => {}
            c => escaped.push(c),
        }
    }
    escaped
}
