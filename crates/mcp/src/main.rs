#![forbid(unsafe_code)]

//! The `bld-mcp` binary: a stdio Model Context Protocol server. It reads
//! newline-delimited JSON-RPC 2.0 requests on stdin and writes one response line
//! per request on stdout. All behaviour lives in the library ([`bld_mcp::dispatch`]).

use std::io::{self, BufRead as _, Write as _};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(request) => bld_mcp::dispatch(&request),
            Err(_) => Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": "parse error" },
            })),
        };
        if let Some(response) = response {
            if writeln!(out, "{response}").is_err() {
                break;
            }
            let _ = out.flush();
        }
    }
}
