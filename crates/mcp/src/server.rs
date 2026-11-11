//! Serving the tools to an AI client with `rmcp`, the official SDK, which
//! speaks both protocol eras (the stateless 2026-07-28 revision and the
//! `initialize` handshake of 2025-11-25 and before).
//!
//! The SDK runs on its own thread with a single-threaded `tokio` runtime.
//! It lists tools and checks arguments itself; each call that passes is
//! handed to the engine as a [`Job`] over a channel, and the engine (which
//! owns the editor, on its own thread) sends the result back. So nothing in
//! the editor needs to be `Send` or async.

use std::sync::Arc;
use std::sync::mpsc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, GetPromptRequestParams,
    GetPromptResponse, GetPromptResult, Implementation, ListPromptsResult,
    ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    Prompt, PromptArgument, PromptMessage, ReadResourceRequestParams, ReadResourceResponse,
    ReadResourceResult, Resource, ResourceContents, ResourceTemplate, Role, ServerCapabilities,
    ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::args::Args;
use crate::error::ToolError;
use crate::extras::{PromptSpec, ResourceSpec};
use crate::spec::{Class, ToolSpec};

/// One tool call for the engine to run.
#[derive(Debug)]
pub struct Call {
    pub tool: &'static ToolSpec,
    pub args: Args,
    /// The client's self-reported name (`clientInfo.name`), for undo labels
    /// and the log. Not proof of anything.
    pub client: Option<String>,
}

/// What a call returns: the text the model reads, or an error.
pub type Outcome = Result<String, ToolError>;

/// A [`Call`] and where to send its [`Outcome`].
#[derive(Debug)]
pub struct Job {
    pub call: Call,
    pub reply: oneshot::Sender<Outcome>,
}

impl Job {
    /// Sends the outcome back. The client may have gone, which is fine.
    pub fn answer(self, outcome: Outcome) {
        let _ = self.reply.send(outcome);
    }
}

/// What the server says about itself, and the tools it lists.
#[derive(Clone, Debug)]
pub struct Setup {
    pub version: String,
    /// Sent to clients at the start; short, because some put it in every prompt.
    pub instructions: String,
    /// In the order they're listed (a fixed order keeps clients' caches warm).
    pub tools: Vec<&'static ToolSpec>,
    pub prompts: Vec<&'static PromptSpec>,
    /// Listed only if the tool each mirrors is among `tools`.
    pub resources: Vec<&'static ResourceSpec>,
    /// Called after each job is sent, so an engine that waits for
    /// something else (the editor's window, between frames) looks at once.
    pub waker: Option<Waker>,
}

/// Wakes the engine when a job arrives (the editor asks for a repaint).
#[derive(Clone)]
pub struct Waker(pub Arc<dyn Fn() + Send + Sync>);

impl std::fmt::Debug for Waker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Waker")
    }
}

impl Setup {
    /// The resources whose tool this server lists.
    fn resources(&self) -> impl Iterator<Item = &'static ResourceSpec> + '_ {
        self.resources
            .iter()
            .copied()
            .filter(|resource| self.tool(resource.tool).is_some())
    }

    fn tool(&self, name: &str) -> Option<&'static ToolSpec> {
        self.tools.iter().copied().find(|spec| spec.name == name)
    }
}

/// The tool as `rmcp` sends it, with annotations from its class: clients
/// skip confirmation for read-only tools and always ask for destructive ones.
///
/// Hints that only restate the spec's defaults are left out, since every
/// listed byte costs tokens: `readOnlyHint` and `idempotentHint` default to
/// false, and `destructiveHint` and `idempotentHint` mean nothing on a
/// read-only tool. `destructiveHint` is always said for one that changes
/// things (its default, true, would be wrong for most), and
/// `openWorldHint` (default true) is always false: the tools touch only the
/// project.
pub fn rmcp_tool(spec: &ToolSpec) -> Tool {
    // The title is the tool's own; not repeated in the annotations.
    let mut annotations = ToolAnnotations::new();
    if spec.class == Class::Read {
        annotations = annotations.read_only(true);
    } else {
        annotations = annotations.destructive(spec.class == Class::Destructive);
        if spec.idempotent {
            annotations = annotations.idempotent(true);
        }
    }
    let annotations = annotations.open_world(false);
    Tool::new(spec.name, spec.summary, Arc::new(spec.input_schema()))
        .with_title(spec.title)
        .with_annotations(annotations)
}

struct Handler {
    setup: Arc<Setup>,
    tools: Arc<Vec<Tool>>,
    jobs: mpsc::Sender<Job>,
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_prompts()
                .enable_resources()
                .enable_tools()
                .build(),
        )
        .with_server_info(
            Implementation::new("axcenity", self.setup.version.clone())
                .with_title("Axcenity Engine"),
        )
        .with_instructions(self.setup.instructions.clone())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.as_ref().clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let Some(&tool) = self
            .setup
            .tools
            .iter()
            .find(|spec| spec.name == request.name)
        else {
            // An unknown tool is a protocol error (the spec's rule).
            return Err(ErrorData::invalid_params(
                format!("unknown tool: {}", request.name),
                None,
            ));
        };
        let args = Args::new(request.arguments.unwrap_or_default());
        if let Err(error) = args.check(tool) {
            return Ok(failed(&error).into());
        }
        let client = context.client_info().map(|info| info.name);
        Ok(match self.run(tool, args, client).await? {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(error) => failed(&error),
        }
        .into())
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        let prompts = self
            .setup
            .prompts
            .iter()
            .map(|spec| {
                let args = spec
                    .args
                    .iter()
                    .map(|arg| {
                        PromptArgument::new(arg.name)
                            .with_description(arg.about)
                            .with_required(arg.required)
                    })
                    .collect();
                Prompt::new(spec.name, Some(spec.description), Some(args)).with_title(spec.title)
            })
            .collect();
        Ok(ListPromptsResult::with_all_items(prompts))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        let Some(spec) = self
            .setup
            .prompts
            .iter()
            .find(|spec| spec.name == request.name)
        else {
            return Err(ErrorData::invalid_params(
                format!("unknown prompt: {}", request.name),
                None,
            ));
        };
        let text = spec
            .message(request.arguments.unwrap_or_default(), &self.setup.tools)
            .map_err(|why| ErrorData::invalid_params(why, None))?;
        Ok(
            GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
                .with_description(spec.description)
                .into(),
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let resources = self
            .setup
            .resources()
            .filter(|spec| spec.template().is_none())
            .map(|spec| {
                Resource::new(spec.uri, spec.name)
                    .with_title(spec.title)
                    .with_description(spec.description)
                    .with_mime_type("text/plain")
            })
            .collect();
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        let templates = self
            .setup
            .resources()
            .filter(|spec| spec.template().is_some())
            .map(|spec| {
                ResourceTemplate::new(spec.uri, spec.name)
                    .with_title(spec.title)
                    .with_description(spec.description)
                    .with_mime_type("text/plain")
            })
            .collect();
        Ok(ListResourceTemplatesResult::with_all_items(templates))
    }

    /// Reads a resource by running the tool it mirrors.
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let found = self.setup.resources().find_map(|spec| {
            let args = spec.args_for(&request.uri)?;
            Some((self.setup.tool(spec.tool)?, args))
        });
        let Some((tool, args)) = found else {
            return Err(ErrorData::resource_not_found(
                format!("no resource {}", request.uri),
                None,
            ));
        };
        let args = Args::new(args);
        if let Err(error) = args.check(tool) {
            return Err(ErrorData::invalid_params(error.to_string(), None));
        }
        let client = context.client_info().map(|info| info.name);
        match self.run(tool, args, client).await? {
            Ok(text) => {
                Ok(ReadResourceResult::new(vec![ResourceContents::text(text, request.uri)]).into())
            }
            Err(error) => Err(ErrorData::resource_not_found(error.to_string(), None)),
        }
    }
}

impl Handler {
    /// Hands one call to the engine and waits for its outcome. The error is
    /// for an engine that has stopped.
    async fn run(
        &self,
        tool: &'static ToolSpec,
        args: Args,
        client: Option<String>,
    ) -> Result<Outcome, ErrorData> {
        let (reply, answer) = oneshot::channel();
        let job = Job {
            call: Call { tool, args, client },
            reply,
        };
        if self.jobs.send(job).is_err() {
            return Err(ErrorData::internal_error("the editor has stopped", None));
        }
        if let Some(waker) = &self.setup.waker {
            (waker.0)();
        }
        answer
            .await
            .map_err(|_| ErrorData::internal_error("the editor stopped before answering", None))
    }
}

fn failed(error: &ToolError) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(error.to_string())])
}

/// Serves `setup` over `transport` until the client goes, sending calls to
/// `jobs`. For tests and other transports; [`serve_stdio`] is the usual one.
pub async fn serve<T, E, A>(
    setup: Setup,
    jobs: mpsc::Sender<Job>,
    transport: T,
) -> std::io::Result<()>
where
    T: rmcp::transport::IntoTransport<RoleServer, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    let tools = setup.tools.iter().map(|spec| rmcp_tool(spec)).collect();
    let handler = Handler {
        setup: Arc::new(setup),
        tools: Arc::new(tools),
        jobs,
    };
    let running = handler
        .serve(transport)
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    running
        .waiting()
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(())
}

/// Serves `setup` on this process's stdin and stdout until the client
/// closes stdin, on a thread of its own. When it returns, `jobs`' sender
/// is dropped, so the engine's receive loop ends too.
pub fn serve_stdio(
    setup: Setup,
    jobs: mpsc::Sender<Job>,
) -> std::thread::JoinHandle<std::io::Result<()>> {
    std::thread::Builder::new()
        .name("mcp".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(serve(setup, jobs, rmcp::transport::stdio()))
        })
        .expect("start the MCP thread")
}

/// Serves `setup` on a TCP connection (the editor's loopback listener,
/// after its handshake) until either side closes it, on the calling
/// thread. When it returns, `jobs`' sender is dropped.
pub fn serve_tcp(
    setup: Setup,
    jobs: mpsc::Sender<Job>,
    stream: std::net::TcpStream,
) -> std::io::Result<()> {
    stream.set_nonblocking(true)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let stream = tokio::net::TcpStream::from_std(stream)?;
        serve(setup, jobs, stream.into_split()).await
    })
}

/// The JSON text clients receive for `tools/list` in this setup, for
/// measuring its size.
pub fn tools_list_json(tools: &[&'static ToolSpec]) -> String {
    let tools: Vec<Value> = tools
        .iter()
        .map(|spec| serde_json::to_value(rmcp_tool(spec)).unwrap_or(Value::Null))
        .collect();
    Value::Array(tools).to_string()
}
