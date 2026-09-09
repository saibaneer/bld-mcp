//! Human-readable reporting of an [`Analysis`] — shared by the CLI and the MCP
//! server so both speak with one voice.

use crate::topology::{Analysis, EdgeKind, Severity};
use std::fmt::Write as _;

/// The per-door shape of a spec plus every diagnostic, as a printable block.
#[must_use]
pub fn human(analysis: &Analysis) -> String {
    let topology = &analysis.topology;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "domain: {}   ({} states, initial: {})",
        topology.domain,
        topology.states.len(),
        topology.initial,
    );
    for door in &topology.doors {
        let total = door.cells.len();
        let no_edge = door
            .cells
            .iter()
            .filter(|c| c.kind == EdgeKind::NoEdge)
            .count();
        let fixed = if door.fixed_table {
            "  [fixed_table]"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "  {:<13} {:>2} inputs   {:>2} edges   {:>2} no_edge{fixed}",
            door.name,
            door.inputs.len(),
            total - no_edge,
            no_edge,
        );
    }

    append_section(&mut out, analysis, Severity::Warning, "warnings");
    append_section(&mut out, analysis, Severity::Error, "errors");
    out
}

fn append_section(out: &mut String, analysis: &Analysis, severity: Severity, heading: &str) {
    let messages: Vec<&str> = analysis
        .diagnostics
        .iter()
        .filter(|d| d.severity == severity)
        .map(|d| d.message.as_str())
        .collect();
    if messages.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n{heading}:");
    for message in messages {
        let _ = writeln!(out, "  - {message}");
    }
}

/// The one-line verdict a caller keys its exit code (or its report) on.
#[must_use]
pub fn verdict(analysis: &Analysis) -> String {
    if analysis.is_sound() {
        format!(
            "VALID — the topology is total ({} no_edge cells made explicit) and sound.",
            analysis.no_edge_count()
        )
    } else {
        format!(
            "INVALID — {} error(s). The topology is not sound.",
            analysis.error_count()
        )
    }
}
