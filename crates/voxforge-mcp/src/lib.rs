// `chunks_exact_to_as_chunks` was added in Rust 1.99 clippy; older stable versions don't know
// this lint. `unknown_lints` silences the "unknown lint" warning on older compilers so the lint
// allow doesn't itself become a `-D warnings` failure.
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

//! Model Context Protocol (MCP) server implementation for VoxForge.
//!
//! Exposes VoxForge speech synthesis, voice cloning, audio transcription, model
//! catalog, and DAG pipeline capabilities directly to AI agents (Cursor, Claude Desktop)
//! via standard JSON-RPC 2.0 over standard input and output streams.

pub mod protocol;
pub mod security;
pub mod server;
pub mod tools;

pub use protocol::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpPrompt, McpResource, McpResourceContent,
    McpTool, ToolCallResult,
};
pub use server::McpServer;
pub use tools::get_mcp_tools;
