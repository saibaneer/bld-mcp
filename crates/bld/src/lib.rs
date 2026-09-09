#![forbid(unsafe_code)]

//! BLD tooling: parse a domain spec, derive and validate its topology, and render
//! it. The `bld` binary is a thin CLI over these modules; the (later) MCP server
//! wraps the same binary, so this library is the single source of behaviour.

pub mod render;
pub mod report;
pub mod scaffold;
pub mod spec;
pub mod topology;
