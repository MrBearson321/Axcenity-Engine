//! `axcenity-editor --mcp` run as an AI client starts it: the real binary,
//! JSON-RPC lines on stdin and stdout. Checks that stdout carries nothing
//! but protocol messages and that the server exits when stdin closes.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A copy of `examples/<name>`, so the server can't change the original.
fn copy_of_example(name: &str) -> PathBuf {
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    let folder =
        std::env::temp_dir().join(format!("axcenity-mcp-stdio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    copy_folder(&example, &folder);
    folder
}

#[test]
fn a_client_lists_and_calls_tools_over_stdio() {
    let folder = copy_of_example("hello-part");
    let mut child = Command::new(env!("CARGO_BIN_EXE_axcenity-editor"))
        .arg("--mcp")
        .arg(&folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut send = |message: Value| {
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    };
    // Every stdout line is a JSON-RPC message; the next one answering `id`.
    let mut answer = |id: u64| -> Value {
        loop {
            let mut line = String::new();
            assert!(
                stdout.read_line(&mut line).unwrap() > 0,
                "stdout closed before answer {id}"
            );
            let message: Value = serde_json::from_str(&line)
                .unwrap_or_else(|_| panic!("stdout line isn't JSON: {line:?}"));
            assert_eq!(message["jsonrpc"], "2.0", "{line}");
            if message["id"] == id {
                return message;
            }
        }
    };

    send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "stdio-test", "version": "1"}
        }}),
    );
    let init = answer(1);
    assert_eq!(init["result"]["serverInfo"]["name"], "axcenity", "{init}");
    send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let list = answer(2);
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    for name in [
        "project_info",
        "scene_inspect",
        "object_get",
        "docs",
        "object_create",
        "object_set",
        "object_delete",
        "history",
        "project_save",
    ] {
        assert!(names.contains(&name), "{names:?}");
    }

    send(
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
            "name": "object_get", "arguments": {"id": "Workspace/Block"}
        }}),
    );
    let call = answer(3);
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Part \"Block\""), "{call}");
    assert_ne!(call["result"]["isError"], true, "{call}");

    send(
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {
            "name": "object_get", "arguments": {"id": "Workspace/Blok"}
        }}),
    );
    let wrong = answer(4);
    assert_eq!(wrong["result"]["isError"], true, "{wrong}");
    let text = wrong["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("did you mean \"Workspace/Block\"?"), "{text}");

    // A write is an undo step named after the client.
    send(
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {
            "name": "object_create", "arguments": {"kind": "Block", "name": "Wall"}
        }}),
    );
    let created = answer(5);
    let text = created["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.ends_with("undo \"AI (stdio-test): Create Wall\""),
        "{created}"
    );

    // Closing stdin ends the session; the server exits cleanly.
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success(), "{status}");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_folder_that_isnt_a_project_fails_without_touching_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_axcenity-editor"))
        .args(["--mcp", "/no/such/axcenity/project"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not open the project"));
}
