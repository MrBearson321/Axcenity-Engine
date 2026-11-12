//! [`ScriptRuntime`](crate::ScriptRuntime)'s methods, split by what they
//! serve. The struct, its constructor, starting and reloading scripts, the
//! camera and graphics settings and the output queues stay in `lib.rs`; each
//! file here is another `impl ScriptRuntime` block.

mod chat;
mod console;
mod debugging;
mod frame;
mod players;
