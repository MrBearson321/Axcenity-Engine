//! The editor's MCP server (Model Context Protocol), so AI clients such as
//! Claude Code, Cursor or Codex can inspect and edit a project through a
//! small set of tools. Editor-only: the player never depends on it.
//!
//! This crate knows nothing about the engine. It defines what a tool is
//! ([`ToolSpec`], with its safety [`Class`] and [`Profile`]), how a call's
//! arguments are checked ([`Args`]) and how errors read ([`ToolError`]), and
//! serves the tools with `rmcp` ([`serve_stdio`]). The editor implements the
//! tools (`editor/src/app/mcp/`) and runs each [`Job`] on its own thread.
//! Design: `docs/research/mcp/architecture.md`; behavior: `docs/editor/mcp.md`.

mod args;
mod error;
mod safety;
mod server;
mod spec;
pub mod text;

pub use args::{Args, Detail};
pub use error::{Code, ToolError, closest};
pub use safety::{Allowed, Confirms, Mode, call_key};
pub use server::{Call, Job, Outcome, Setup, rmcp_tool, serve, serve_stdio, tools_list_json};
pub use spec::{Class, Kind, Param, Profile, ToolSpec};
