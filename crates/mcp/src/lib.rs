//! FigCraft's MCP server.
//!
//! [Model Context Protocol](https://modelcontextprotocol.io) over stdio: newline-delimited JSON-RPC
//! 2.0, hand-written (no async runtime). The server exposes FigCraft as MCP tools, resources and
//! prompts, and forwards everything to a [`Backend`]:
//!
//! - [`Headless`] hosts an in-process [`figcraft_engine::Session`] and answers the control-channel
//!   methods itself (rendering with `figcraft-render`), so agents can design without a window.
//! - [`Attached`] talks to a running desktop app through its loopback JSON-lines control channel
//!   (`figcraft --control PORT`): one `{"id","method","params"}` line in, one
//!   `{"id","ok","result"|"error"}` line out.
//!
//! Entry points: [`Server::serve`] (stdio loop) and [`Server::handle_line`] (one message).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

/// The product's name as this server gives it (`get_capabilities.product`, `serverInfo.title`):
/// one constant, so a rename changes it in one place.
pub const PRODUCT_NAME: &str = "FigCraft";

mod backend;
mod headless;
mod prompts;
mod resources;
mod server;
mod tools;

#[cfg(test)]
mod gradient_tests;

pub use backend::{Attached, Backend, decode_data, error_detail, page_children};
pub use headless::Headless;
pub use prompts::{PROMPTS, PromptArg, PromptDef};
pub use resources::{COMMAND_TEMPLATE, COMMANDS_URI, DESIGN_URI, DOC_URI, FINDINGS_URI, FLOW_URI, NODE_TEMPLATE, SCENARIOS_URI};
pub use server::{INSTRUCTIONS, PROTOCOL_VERSION, SUPPORTED_VERSIONS, Server};
pub use tools::{TOOL_NAMES, ToolResult, ToolState, call_tool, call_tool_in, tool_definitions};

/// Width and height of a PNG from its IHDR chunk.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..8)? != b"\x89PNG\r\n\x1a\n" || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let h = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    Some((w, h))
}
