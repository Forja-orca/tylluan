//! # Guild Registry Module
//!
//! Manages the lifecycle of Python guild subprocesses:
//! - **Spawn**: Launch Python guilds as child processes communicating via MCP stdio
//! - **Proxy**: Forward MCP `list_tools` and `call_tool` to the appropriate guild
//! - **Lifecycle**: Auto-unload guilds after inactivity timeout

pub mod tools;
pub mod guild_process;
pub mod supervisor;
pub mod lifecycle;
pub mod actor;
pub mod proxy;
pub mod service_manager;
pub mod approval;
