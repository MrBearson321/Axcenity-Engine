//! The server speaks MCP to a client over an in-memory pipe, in both
//! protocol eras: the 2025-11-25 `initialize` handshake most clients still
//! use, and the stateless 2026-07-28 revision (`server/discover`, `_meta` on
//! every request). The "engine" here is a thread that echoes calls.

use std::sync::mpsc;
use std::time::Duration;

use axcenity_mcp::{
    Args, Class, Job, Kind, Param, Profile, PromptArg, PromptSpec, ResourceSpec, Setup, ToolError,
    ToolSpec, serve,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines};

static LOOK: ToolSpec = ToolSpec {
    name: "scene_inspect",
    title: "Inspect scene",
    summary: "Browse or search the scene.",
    profile: Profile::Lite,
    class: Class::Read,
    idempotent: true,
    params: &[Param::optional("root", Kind::String, "object id or path")],
};

static BREAK: ToolSpec = ToolSpec {
    name: "object_delete",
    title: "Delete objects",
    summary: "Delete objects.",
    profile: Profile::Lite,
    class: Class::Destructive,
    idempotent: false,
    params: &[Param::required("ids", Kind::Strings, "objects")],
};

static BUILD: PromptSpec = PromptSpec {
    name: "build",
    title: "Build something",
    description: "Steps for building what you describe.",
    args: &[PromptArg {
        name: "goal",
        about: "what to build",
        required: true,
    }],
    render: build_text,
};

fn build_text(args: &Args, tools: &[&'static ToolSpec]) -> String {
    format!(
        "Build {} with {} tools.",
        args.str("goal").unwrap_or_default(),
        tools.len()
    )
}

static SCENE: ResourceSpec = ResourceSpec {
    uri: "test://scene",
    name: "scene",
    title: "Scene",
    description: "The scene tree.",
    tool: "scene_inspect",
};

static UNDER: ResourceSpec = ResourceSpec {
    uri: "test://under/{root}",
    name: "under",
    title: "Under an object",
    description: "The tree under one object.",
    tool: "scene_inspect",
};

/// Mirrors a tool this server doesn't list, so it isn't listed either.
static HIDDEN: ResourceSpec = ResourceSpec {
    uri: "test://hidden",
    name: "hidden",
    title: "Hidden",
    description: "Not listed.",
    tool: "docs",
};

struct Client {
    to_server: DuplexStream,
    from_server: Lines<BufReader<DuplexStream>>,
    next_id: u64,
}

impl Client {
    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.to_server.write_all(line.as_bytes()).await.unwrap();
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        loop {
            let line = tokio::time::timeout(Duration::from_secs(10), self.from_server.next_line())
                .await
                .expect("the server answers")
                .unwrap()
                .expect("the server is still there");
            let message: Value = serde_json::from_str(&line).expect("every line is JSON");
            if message["id"] == json!(id) {
                return message;
            }
        }
    }
}

/// Starts the server with LOOK and BREAK, and an engine thread that answers
/// every call with the tool's name, the client's name and its arguments (or
/// an error for `root: "nowhere"`). Two one-way pipes make the connection:
/// the client writes to `to_server` and reads what the server writes.
fn start_pair() -> Client {
    let (to_server, server_reads) = tokio::io::duplex(64 * 1024);
    let (server_writes, from_server) = tokio::io::duplex(64 * 1024);
    let (jobs, engine) = mpsc::channel::<Job>();
    std::thread::spawn(move || {
        for job in engine {
            let Job { call, reply } = job;
            let outcome = if call.args.str("root") == Some("nowhere") {
                Err(ToolError::not_found("no object \"nowhere\"")
                    .with_fix("scene_inspect lists objects"))
            } else {
                Ok(format!(
                    "{} for {} with {}",
                    call.tool.name,
                    call.client.as_deref().unwrap_or("?"),
                    Value::Object(call.args.0)
                ))
            };
            let _ = reply.send(outcome);
        }
    });
    let setup = Setup {
        version: "1.0.0".into(),
        instructions: "Use scene_inspect first.".into(),
        tools: vec![&LOOK, &BREAK],
        prompts: vec![&BUILD],
        resources: vec![&SCENE, &UNDER, &HIDDEN],
        waker: None,
    };
    tokio::spawn(async move { serve(setup, jobs, (server_reads, server_writes)).await });
    Client {
        to_server,
        from_server: BufReader::new(from_server).lines(),
        next_id: 0,
    }
}

async fn legacy_client() -> Client {
    let mut client = start_pair();
    let init = client
        .request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "Test Client", "version": "1"}
            }),
        )
        .await;
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25", "{init}");
    assert_eq!(init["result"]["serverInfo"]["name"], "axcenity");
    assert_eq!(init["result"]["instructions"], "Use scene_inspect first.");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(init["result"]["capabilities"]["prompts"].is_object());
    assert!(init["result"]["capabilities"]["resources"].is_object());
    client
        .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        .await;
    client
}

fn text_of(response: &Value) -> &str {
    response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
}

#[tokio::test]
async fn a_2025_client_lists_tools_with_their_annotations_and_flat_schemas() {
    let mut client = legacy_client().await;
    let list = client.request("tools/list", json!({})).await;
    let tools = list["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["scene_inspect", "object_delete"]);
    assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
    // Defaults aren't repeated: a read-only tool's other hints mean nothing.
    assert!(tools[0]["annotations"]["destructiveHint"].is_null());
    assert_eq!(tools[0]["annotations"]["openWorldHint"], false);
    assert_eq!(tools[1]["annotations"]["destructiveHint"], true);
    assert!(tools[1]["annotations"]["readOnlyHint"].is_null());
    assert_eq!(
        tools[1]["inputSchema"],
        json!({
            "type": "object",
            "properties": {"ids": {"type": "array", "items": {"type": "string"}, "description": "objects"}},
            "required": ["ids"]
        })
    );
}

#[tokio::test]
async fn a_2025_client_calls_a_tool_and_the_engine_sees_its_name_and_arguments() {
    let mut client = legacy_client().await;
    let call = client
        .request(
            "tools/call",
            json!({"name": "scene_inspect", "arguments": {"root": "Workspace"}}),
        )
        .await;
    assert_ne!(call["result"]["isError"], true, "{call}");
    assert_eq!(
        text_of(&call),
        r#"scene_inspect for Test Client with {"root":"Workspace"}"#
    );
}

#[tokio::test]
async fn errors_are_tool_results_a_model_can_read() {
    let mut client = legacy_client().await;
    // From the engine.
    let call = client
        .request(
            "tools/call",
            json!({"name": "scene_inspect", "arguments": {"root": "nowhere"}}),
        )
        .await;
    assert_eq!(call["result"]["isError"], true);
    assert_eq!(
        text_of(&call),
        "error not_found: no object \"nowhere\"\nfix: scene_inspect lists objects"
    );
    // From checking the arguments, before the engine sees the call.
    let call = client
        .request(
            "tools/call",
            json!({"name": "object_delete", "arguments": {"ids": "17"}}),
        )
        .await;
    assert_eq!(call["result"]["isError"], true);
    assert!(
        text_of(&call).starts_with("error bad_args: ids should be a list of strings"),
        "{call}"
    );
    let call = client
        .request(
            "tools/call",
            json!({"name": "object_delete", "arguments": {}}),
        )
        .await;
    assert_eq!(
        text_of(&call),
        "error bad_args: object_delete needs ids (a list of strings, like [\"17\",\"Workspace/Door\"])"
    );
    // An unknown tool is a protocol error.
    let call = client
        .request("tools/call", json!({"name": "run_lua", "arguments": {}}))
        .await;
    assert_eq!(call["error"]["code"], -32602, "{call}");
}

#[tokio::test]
async fn a_2026_client_discovers_the_server_and_calls_without_a_handshake() {
    let mut client = start_pair();
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "Modern Client", "version": "2"}
    });
    let discover = client
        .request("server/discover", json!({"_meta": meta}))
        .await;
    let supported = discover["result"]["supportedVersions"]
        .as_array()
        .unwrap_or_else(|| panic!("{discover}"));
    assert!(supported.contains(&json!("2026-07-28")));
    assert!(supported.contains(&json!("2025-11-25")));

    let list = client.request("tools/list", json!({"_meta": meta})).await;
    assert_eq!(
        list["result"]["tools"].as_array().map(Vec::len),
        Some(2),
        "{list}"
    );
    let call = client
        .request(
            "tools/call",
            json!({"_meta": meta, "name": "scene_inspect", "arguments": {}}),
        )
        .await;
    assert_eq!(call["result"]["resultType"], "complete", "{call}");
    assert_eq!(text_of(&call), "scene_inspect for Modern Client with {}");
}

#[tokio::test]
async fn prompts_are_listed_and_filled_in_with_their_arguments() {
    let mut client = legacy_client().await;
    let list = client.request("prompts/list", json!({})).await;
    let prompt = &list["result"]["prompts"][0];
    assert_eq!(prompt["name"], "build", "{list}");
    assert_eq!(prompt["arguments"][0]["name"], "goal");
    assert_eq!(prompt["arguments"][0]["required"], true);
    let got = client
        .request(
            "prompts/get",
            json!({"name": "build", "arguments": {"goal": "a hut"}}),
        )
        .await;
    let message = &got["result"]["messages"][0];
    assert_eq!(message["role"], "user", "{got}");
    assert_eq!(message["content"]["text"], "Build a hut with 2 tools.");
    let missing = client
        .request("prompts/get", json!({"name": "build", "arguments": {}}))
        .await;
    assert_eq!(missing["error"]["code"], -32602, "{missing}");
    assert!(
        missing["error"]["message"]
            .as_str()
            .unwrap()
            .contains("needs goal"),
        "{missing}"
    );
}

#[tokio::test]
async fn resources_mirror_read_tools_and_reading_one_runs_its_tool() {
    let mut client = legacy_client().await;
    let list = client.request("resources/list", json!({})).await;
    let uris: Vec<&str> = list["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert_eq!(uris, ["test://scene"], "{list}");
    let templates = client.request("resources/templates/list", json!({})).await;
    assert_eq!(
        templates["result"]["resourceTemplates"][0]["uriTemplate"], "test://under/{root}",
        "{templates}"
    );
    let read = client
        .request(
            "resources/read",
            json!({"uri": "test://under/Workspace%2FHouse"}),
        )
        .await;
    let contents = &read["result"]["contents"][0];
    assert_eq!(contents["uri"], "test://under/Workspace%2FHouse", "{read}");
    assert_eq!(
        contents["text"],
        r#"scene_inspect for Test Client with {"root":"Workspace/House"}"#
    );
    // The engine's error, and URIs that aren't resources here.
    for uri in ["test://under/nowhere", "test://hidden", "test://other"] {
        let read = client.request("resources/read", json!({"uri": uri})).await;
        assert!(read["error"]["message"].is_string(), "{uri}: {read}");
    }
}

/// The open editor serves each relayed client on a TCP connection, and
/// the waker is called for each job so it looks between frames.
#[test]
fn a_tcp_connection_is_served_and_each_call_wakes_the_engine() {
    use std::io::{BufRead, Write};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let woken = Arc::new(AtomicUsize::new(0));
    let (jobs, engine) = mpsc::channel::<Job>();
    let setup = Setup {
        version: "1.0.0".into(),
        instructions: String::new(),
        tools: vec![&LOOK],
        prompts: Vec::new(),
        resources: Vec::new(),
        waker: Some(axcenity_mcp::Waker(Arc::new({
            let woken = woken.clone();
            move || {
                woken.fetch_add(1, Ordering::SeqCst);
            }
        }))),
    };
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        axcenity_mcp::serve_tcp(setup, jobs, stream)
    });
    std::thread::spawn(move || {
        for job in engine {
            let name = job.call.tool.name;
            job.answer(Ok(format!("ran {name}")));
        }
    });
    let stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let writer = stream.try_clone().unwrap();
    let mut lines = std::io::BufReader::new(stream).lines();
    let send = |message: Value| writeln!(&writer, "{message}").unwrap();
    send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-11-25", "capabilities": {},
        "clientInfo": {"name": "Relay", "version": "1"}}}),
    );
    let init: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "axcenity", "{init}");
    send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "scene_inspect", "arguments": {}}}));
    let answer: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(answer["result"]["content"][0]["text"], "ran scene_inspect");
    assert_eq!(woken.load(Ordering::SeqCst), 1);
    // The client hangs up: the server ends.
    drop(lines);
    writer.shutdown(std::net::Shutdown::Both).unwrap();
    server.join().unwrap().unwrap();
}
