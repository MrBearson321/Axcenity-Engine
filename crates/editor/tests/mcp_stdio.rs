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
    copy_of_example_for(name, "stdio")
}

/// A copy of `examples/<name>` of this test's own.
fn copy_of_example_for(name: &str, test: &str) -> PathBuf {
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    let folder =
        std::env::temp_dir().join(format!("axcenity-mcp-{test}-{name}-{}", std::process::id()));
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

#[test]
fn check_prints_a_report_and_client_lines_for_this_binary() {
    let folder = copy_of_example("hello-touched");
    let output = Command::new(env!("CARGO_BIN_EXE_axcenity-editor"))
        .args(["--mcp", "--check"])
        .arg(&folder)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&folder);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{text}");
    assert!(
        text.starts_with("Axcenity MCP check\nok   project "),
        "{text}"
    );
    let exe = std::fs::canonicalize(env!("CARGO_BIN_EXE_axcenity-editor")).unwrap();
    assert!(text.contains(&format!("{}", exe.display())), "{text}");
    assert!(text.contains("claude mcp add axcenity -- "), "{text}");
    let missing = Command::new(env!("CARGO_BIN_EXE_axcenity-editor"))
        .args(["--mcp", "--check", "/no/such/project"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
}

/// Stands in for an open editor that takes AI connections for `folder`:
/// writes its endpoint file and answers each hello with `answer(hello)`.
/// After an `ok` to a client (not a probe), echoes the client's first
/// message's id back with `{"relayed": true}`. Each hello it heard is sent
/// to the returned channel.
fn fake_editor(
    folder: &Path,
    answer: fn(&Value) -> &'static str,
) -> std::sync::mpsc::Receiver<Value> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::fs::create_dir_all(folder.join(".axcenity")).unwrap();
    std::fs::write(
        folder.join(".axcenity/mcp-endpoint.json"),
        json!({"port": port, "token": "secret", "pid": 1}).to_string(),
    )
    .unwrap();
    let (heard, hellos) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut lines = BufReader::new(stream.try_clone().unwrap()).lines();
            let Some(Ok(line)) = lines.next() else {
                continue;
            };
            let hello: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(hello["token"], "secret");
            let reply = answer(&hello);
            writeln!(stream, "{reply}").unwrap();
            let probe = hello["probe"] == true;
            let _ = heard.send(hello);
            if reply == "ok" && !probe {
                let Some(Ok(line)) = lines.next() else {
                    continue;
                };
                let message: Value = serde_json::from_str(&line).unwrap();
                let echo =
                    json!({"jsonrpc": "2.0", "id": message["id"], "result": {"relayed": true}});
                writeln!(stream, "{echo}").unwrap();
            }
        }
    });
    hellos
}

#[test]
fn the_server_relays_to_an_open_editor_with_its_options() {
    let folder = copy_of_example_for("hello-part", "relay");
    let hellos = fake_editor(&folder, |_| "ok");
    let mut child = Command::new(env!("CARGO_BIN_EXE_axcenity-editor"))
        .args([
            "--mcp".as_ref(),
            folder.as_os_str(),
            "--mode".as_ref(),
            "read".as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 7, "method": "initialize", "params": {}})
    )
    .unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let answer: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        answer,
        json!({"jsonrpc": "2.0", "id": 7, "result": {"relayed": true}})
    );
    let hello = hellos.recv().unwrap();
    assert_eq!(hello["mode"], "read");
    assert_eq!(hello["profile"], "standard");
    assert_eq!(hello["probe"], false);
    // The editor hung up: the relay ends.
    drop(stdin);
    assert!(child.wait().unwrap().success());
    // Relaying, it never opened the project itself.
    assert!(!folder.join(".axcenity/mcp-log.jsonl").exists());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn without_the_editor_the_server_refuses_writes_while_an_editor_listens() {
    let folder = copy_of_example_for("hello-part", "guard");
    // The editor refuses this client but says it's there to a probe.
    let hellos = fake_editor(&folder, |hello| {
        if hello["probe"] == true {
            "ok"
        } else {
            "no busy"
        }
    });
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
    let mut ask = |stdin: &mut std::process::ChildStdin, message: Value| -> Value {
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
        loop {
            let mut line = String::new();
            assert!(stdout.read_line(&mut line).unwrap() > 0);
            let answer: Value = serde_json::from_str(&line).unwrap();
            if answer["id"] == message["id"] {
                return answer;
            }
        }
    };
    let init = ask(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-11-25", "capabilities": {},
        "clientInfo": {"name": "guard-test", "version": "1"}}}),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "axcenity", "{init}");
    assert_eq!(hellos.recv().unwrap()["probe"], false);
    // Served without the window: it marks the project as such.
    assert!(folder.join(".axcenity/mcp-headless.lock").exists());
    let call = |id: u64, tool: &str, args: Value| {
        json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": tool, "arguments": args}})
    };
    let read = ask(&mut stdin, call(2, "project_info", json!({})));
    assert_ne!(read["result"]["isError"], true, "{read}");
    let write = ask(
        &mut stdin,
        call(3, "object_create", json!({"kind": "Block"})),
    );
    assert_eq!(write["result"]["isError"], true, "{write}");
    let text = write["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("the editor has this project open"), "{text}");
    // Closing stdin ends it, and the mark goes with it.
    drop(stdin);
    assert!(child.wait().unwrap().success());
    assert!(!folder.join(".axcenity/mcp-headless.lock").exists());
    let _ = std::fs::remove_dir_all(&folder);
}

/// Windows gives a program's first thread a 1 MB stack, less than a debug
/// build needs to open a project; the server runs on a thread of its own.
/// Here the same limit is set with `ulimit -s` (the Windows build gets it
/// from the executable itself).
#[cfg(unix)]
#[test]
fn the_server_starts_with_a_one_megabyte_main_thread_stack() {
    let folder = copy_of_example_for("hello-part", "small-stack");
    let output = Command::new("sh")
        .arg("-c")
        .arg("ulimit -s 1024 && exec \"$0\" --mcp --check \"$1\"")
        .arg(env!("CARGO_BIN_EXE_axcenity-editor"))
        .arg(&folder)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("Axcenity MCP check"), "{stdout}");
    assert!(!stderr.contains("overflowed"), "{stderr}");
    let _ = std::fs::remove_dir_all(&folder);
}
