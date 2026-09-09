//! Verify that a shipped domain still matches its spec — catch drift.
//!
//! The spec is the published contract. A domain implemented from it exports its
//! *real* topology (see `bld scaffold probe`) as the same `topology.json` shape
//! `bld topology render` emits. This compares the two cell by cell: any pair where
//! the domain's behaviour differs from the spec — a transition the code grew, one
//! it lost, or a target that changed — is drift, reported here. It is the generic
//! form of the reference implementation's committed-topology test.

use crate::topology::{EdgeKind, Topology};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// One `(door, state, input)` cell where the domain and the spec disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub door: String,
    pub from: String,
    pub input: String,
    pub in_spec: String,
    pub in_domain: String,
}

/// The result of a verification: every point of drift (empty means they match).
#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub drifts: Vec<Drift>,
}

impl VerifyReport {
    /// True when the domain matches the spec exactly.
    #[must_use]
    pub fn matches(&self) -> bool {
        self.drifts.is_empty()
    }
}

const ABSENT: &str = "(absent)";

type CellKey = (String, String, String);

/// Compare a spec's derived topology against a domain's exported `topology.json`.
///
/// # Errors
/// The exported value is not a `topology.json` (no `doors`/`cells`/`edge`).
pub fn verify(spec_topology: &Topology, exported: &Value) -> Result<VerifyReport, String> {
    let spec = spec_cells(spec_topology);
    let domain = json_cells(exported)?;

    let keys: BTreeSet<CellKey> = spec.keys().chain(domain.keys()).cloned().collect();
    let mut drifts = Vec::new();
    for key in keys {
        let in_spec = spec.get(&key).map_or(ABSENT, String::as_str);
        let in_domain = domain.get(&key).map_or(ABSENT, String::as_str);
        if in_spec != in_domain {
            drifts.push(Drift {
                door: key.0,
                from: key.1,
                input: key.2,
                in_spec: in_spec.to_owned(),
                in_domain: in_domain.to_owned(),
            });
        }
    }
    Ok(VerifyReport { drifts })
}

/// A human-readable drift report.
#[must_use]
pub fn report(report: &VerifyReport) -> String {
    use std::fmt::Write as _;
    if report.matches() {
        return "MATCHES — the domain's topology is identical to the spec.\n".to_owned();
    }
    let mut out = format!(
        "DRIFT — the domain disagrees with the spec in {} cell(s):\n",
        report.drifts.len()
    );
    for drift in &report.drifts {
        let _ = writeln!(
            out,
            "  {} ({}, {}): spec = {}, domain = {}",
            drift.door, drift.from, drift.input, drift.in_spec, drift.in_domain
        );
    }
    out
}

fn spec_cells(topology: &Topology) -> BTreeMap<CellKey, String> {
    let mut map = BTreeMap::new();
    for door in &topology.doors {
        for cell in &door.cells {
            map.insert(
                (door.name.to_owned(), cell.from.clone(), cell.input.clone()),
                edge_kind(&cell.kind),
            );
        }
    }
    map
}

fn edge_kind(kind: &EdgeKind) -> String {
    match kind {
        EdgeKind::NoEdge => "no_edge".to_owned(),
        EdgeKind::Local { to } => format!("local -> {to}"),
        EdgeKind::External { to, effect } => format!("external -> {to} / {effect}"),
        // A system_event's `records` label is spec-only documentation — the kernel's
        // `SystemEventResolution::Record` carries none — so compare on existence.
        EdgeKind::Record { .. } => "record".to_owned(),
    }
}

fn json_cells(exported: &Value) -> Result<BTreeMap<CellKey, String>, String> {
    let doors = exported
        .get("doors")
        .and_then(Value::as_array)
        .ok_or("exported topology has no `doors` array")?;
    let mut map = BTreeMap::new();
    for door in doors {
        let name = door
            .get("name")
            .and_then(Value::as_str)
            .ok_or("a door is missing its `name`")?;
        let cells = door
            .get("cells")
            .and_then(Value::as_array)
            .ok_or("a door is missing its `cells`")?;
        for cell in cells {
            let from = cell.get("from").and_then(Value::as_str).unwrap_or("");
            let input = cell.get("input").and_then(Value::as_str).unwrap_or("");
            map.insert(
                (name.to_owned(), from.to_owned(), input.to_owned()),
                json_edge_kind(cell)?,
            );
        }
    }
    Ok(map)
}

fn json_edge_kind(cell: &Value) -> Result<String, String> {
    let edge = cell
        .get("edge")
        .and_then(Value::as_str)
        .ok_or("a cell is missing its `edge`")?;
    let field = |key: &str| cell.get(key).and_then(Value::as_str).unwrap_or("");
    Ok(match edge {
        "no_edge" => "no_edge".to_owned(),
        "local" => format!("local -> {}", field("to")),
        "external" => format!("external -> {} / {}", field("to"), field("effect")),
        "record" => "record".to_owned(),
        other => format!("unknown({other})"),
    })
}
