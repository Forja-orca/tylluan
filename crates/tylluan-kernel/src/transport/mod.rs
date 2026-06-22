//! # Transport Module
//!
//! Handles MCP protocol communication over multiple transports:
//! - **Stdio**: For local IDE integration (VSCode, Cursor, Claude Desktop)
//! - **HTTP**: For remote/multi-agent connections (Streamable HTTP + Legacy SSE)
//!
//! Both transports feed into the same ToolRegistry for unified tool execution.

pub mod server;
pub mod http;
pub mod mdns;
