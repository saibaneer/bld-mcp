//! Turn a derived [`Topology`] into shareable artifacts: `topology.json` (the
//! data, in the same shape the town-hall reference publishes) and a Mermaid state
//! diagram (renders inline on GitHub, no tools to install).
//!
//! Both are *derived*, never hand-kept — a change to the spec changes the
//! artifacts, so they cannot drift from the domain they describe.

use crate::topology::{EdgeKind, Topology};
use serde_json::{Value, json};
use std::fmt::Write as _;

const NOTE: &str = "Derived by `bld topology render`. A `no_edge` cell is a transition that does \
not exist, not one refused at runtime. `fixed_table` per door says whether the door is decided \
by (state, input) alone; where it is false the door reads persisted data and the enumeration is \
of the intended shape, not every runtime axis.";

/// The topology as `topology.json` — pretty-printed, stable ordering.
#[must_use]
pub fn to_json(topology: &Topology) -> String {
    let doors: Vec<Value> = topology
        .doors
        .iter()
        .map(|door| {
            let cells: Vec<Value> = door
                .cells
                .iter()
                .map(|cell| match &cell.kind {
                    EdgeKind::Local { to } => json!({
                        "from": cell.from, "input": cell.input, "edge": "local", "to": to,
                    }),
                    EdgeKind::External { to, effect } => json!({
                        "from": cell.from, "input": cell.input, "edge": "external",
                        "to": to, "effect": effect,
                    }),
                    EdgeKind::Record { records } => json!({
                        "from": cell.from, "input": cell.input, "edge": "record",
                        "records": records,
                    }),
                    EdgeKind::NoEdge => json!({
                        "from": cell.from, "input": cell.input, "edge": "no_edge",
                    }),
                })
                .collect();
            json!({
                "name": door.name,
                "fixed_table": door.fixed_table,
                "inputs": door.inputs,
                "cells": cells,
            })
        })
        .collect();

    let doc = json!({
        "generated_by": "bld topology render",
        "note": NOTE,
        "domain": topology.domain,
        "initial": topology.initial,
        "states": topology.states,
        "doors": doors,
    });

    // Infallible: the value is a plain object of strings/arrays.
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

/// The topology as a Mermaid `stateDiagram-v2`. Only state-moving cells (proposal
/// and fact edges) are drawn; a `system_event` records a pursuit decision and
/// moves no state, so it does not appear as a transition.
#[must_use]
pub fn to_mermaid(topology: &Topology) -> String {
    let mut out = String::from("stateDiagram-v2\n");
    let _ = writeln!(out, "    [*] --> {}", topology.initial);

    for door in &topology.doors {
        for cell in &door.cells {
            let (to, label) = match &cell.kind {
                EdgeKind::Local { to } => (to, cell.input.clone()),
                EdgeKind::External { to, effect } => (to, format!("{} / {effect}", cell.input)),
                EdgeKind::Record { .. } | EdgeKind::NoEdge => continue,
            };
            let label = if door.name == "fact" {
                format!("{label} (fact)")
            } else {
                label
            };
            let _ = writeln!(out, "    {} --> {to}: {label}", cell.from);
        }
    }
    out
}
