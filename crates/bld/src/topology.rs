//! Derive the transition topology from a spec, and judge whether it is sound.
//!
//! The derivation fills the **full grid**: for every door, every `(state, input)`
//! pair becomes a cell — an authored edge, or `no_edge`. Totality is therefore a
//! property of construction, not a check that can be forgotten: the artifact
//! cannot omit a pair. What *can* be wrong is the authored content — an edge to a
//! state that does not exist, an input the door does not accept, two edges
//! answering the same `(state, input)`, a runtime fact pretending to move state.
//! Those are the diagnostics.

use crate::spec::{DomainSpec, Door, Edge};
use std::collections::HashSet;

/// What a single `(state, input)` cell resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeKind {
    /// A state-only transition.
    Local { to: String },
    /// A transition that reaches out to the world, carrying an effect.
    External { to: String, effect: String },
    /// A runtime pursuit decision (`system_event` door): records an outcome,
    /// moves no state.
    Record { records: String },
    /// The transition does not exist here.
    NoEdge,
}

/// One cell of one door's grid.
#[derive(Debug, Clone)]
pub struct Cell {
    pub from: String,
    pub input: String,
    pub kind: EdgeKind,
}

/// One door's complete grid: every `(state, input)` pair, in state-major order.
#[derive(Debug, Clone)]
pub struct DoorTopology {
    pub name: &'static str,
    pub fixed_table: bool,
    pub inputs: Vec<String>,
    pub cells: Vec<Cell>,
}

/// The derived topology for a whole domain.
#[derive(Debug, Clone)]
pub struct Topology {
    pub domain: String,
    pub initial: String,
    pub states: Vec<String>,
    pub doors: Vec<DoorTopology>,
}

/// How serious a diagnostic is. An `Error` means the domain is unsound and the
/// artifact must not be trusted; a `Warning` is advisory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// One thing the validator found.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
}

/// The result of analysing a spec: the derived grid, plus every diagnostic.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub topology: Topology,
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    /// How many `Error`-severity diagnostics were found.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    /// How many `Warning`-severity diagnostics were found.
    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    /// A domain is sound when nothing is an error. Warnings do not block it.
    #[must_use]
    pub fn is_sound(&self) -> bool {
        self.error_count() == 0
    }

    /// The number of `no_edge` cells across all doors — the size of the absence
    /// the topology makes explicit.
    #[must_use]
    pub fn no_edge_count(&self) -> usize {
        self.topology
            .doors
            .iter()
            .flat_map(|door| &door.cells)
            .filter(|cell| cell.kind == EdgeKind::NoEdge)
            .count()
    }
}

/// Analyse a spec: derive its full grid and collect every diagnostic.
#[must_use]
pub fn analyze(spec: &DomainSpec) -> Analysis {
    let mut diagnostics = Vec::new();
    let states: HashSet<&str> = spec.states.iter().map(String::as_str).collect();

    check_states(spec, &states, &mut diagnostics);

    let doors = spec
        .doors()
        .into_iter()
        .map(|(name, door)| derive_door(name, door, &spec.states, &states, &mut diagnostics))
        .collect();

    let topology = Topology {
        domain: spec.domain.clone(),
        initial: spec.initial.clone(),
        states: spec.states.clone(),
        doors,
    };

    check_reachability(&topology, &mut diagnostics);

    Analysis {
        topology,
        diagnostics,
    }
}

fn error(diagnostics: &mut Vec<Diagnostic>, message: String) {
    diagnostics.push(Diagnostic {
        severity: Severity::Error,
        message,
    });
}

fn warn(diagnostics: &mut Vec<Diagnostic>, message: String) {
    diagnostics.push(Diagnostic {
        severity: Severity::Warning,
        message,
    });
}

fn check_states(spec: &DomainSpec, states: &HashSet<&str>, diagnostics: &mut Vec<Diagnostic>) {
    if spec.states.is_empty() {
        error(diagnostics, "the domain declares no states".to_owned());
    }
    let mut seen = HashSet::new();
    for state in &spec.states {
        if !seen.insert(state.as_str()) {
            error(diagnostics, format!("state `{state}` is declared twice"));
        }
    }
    if !states.contains(spec.initial.as_str()) {
        error(
            diagnostics,
            format!(
                "initial state `{}` is not one of the declared states",
                spec.initial
            ),
        );
    }
}

/// Build one door's full grid, emitting a diagnostic for every unsound edge.
fn derive_door(
    name: &'static str,
    door: &Door,
    ordered_states: &[String],
    states: &HashSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> DoorTopology {
    let inputs_set: HashSet<&str> = door.inputs.iter().map(String::as_str).collect();
    if door.inputs.is_empty() {
        error(diagnostics, format!("door `{name}` declares no inputs"));
    }

    // (from, input) -> the authored edge; a repeat is two competing answers.
    let mut authored: std::collections::HashMap<(&str, &str), &Edge> =
        std::collections::HashMap::new();
    for edge in &door.edges {
        validate_edge(name, edge, states, &inputs_set, diagnostics);
        if authored
            .insert((edge.from.as_str(), edge.input.as_str()), edge)
            .is_some()
        {
            error(
                diagnostics,
                format!(
                    "door `{name}` has two edges for (`{}`, `{}`) — a cell can hold only one answer",
                    edge.from, edge.input
                ),
            );
        }
    }

    // Fill the grid: state-major, then input, so the artifact is stable and total.
    let mut cells = Vec::with_capacity(ordered_states.len() * door.inputs.len());
    for from in ordered_states {
        for input in &door.inputs {
            let kind = authored
                .get(&(from.as_str(), input.as_str()))
                .map_or(EdgeKind::NoEdge, |edge| edge_kind(name, edge));
            cells.push(Cell {
                from: from.clone(),
                input: input.clone(),
                kind,
            });
        }
    }

    DoorTopology {
        name,
        fixed_table: door.fixed_table,
        inputs: door.inputs.clone(),
        cells,
    }
}

/// Check one authored edge's references and per-door field rules.
fn validate_edge(
    door: &'static str,
    edge: &Edge,
    states: &HashSet<&str>,
    inputs: &HashSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !states.contains(edge.from.as_str()) {
        error(
            diagnostics,
            format!(
                "door `{door}`: edge `from: {}` is not a declared state",
                edge.from
            ),
        );
    }
    if !inputs.contains(edge.input.as_str()) {
        error(
            diagnostics,
            format!(
                "door `{door}`: edge `input: {}` is not one of the door's inputs",
                edge.input
            ),
        );
    }
    if let Some(to) = &edge.to {
        if !states.contains(to.as_str()) {
            error(
                diagnostics,
                format!("door `{door}`: edge `to: {to}` is not a declared state"),
            );
        }
    }

    match door {
        "proposal" => {
            if edge.to.is_none() {
                edge_needs(door, edge, "to", diagnostics);
            }
            if edge.records.is_some() {
                edge_forbids(door, edge, "records", diagnostics);
            }
        }
        "fact" => {
            if edge.to.is_none() {
                edge_needs(door, edge, "to", diagnostics);
            }
            if edge.records.is_some() {
                edge_forbids(door, edge, "records", diagnostics);
            }
            if edge.effect.is_some() {
                // A fact confirms an effect; it does not dispatch one.
                edge_forbids(door, edge, "effect", diagnostics);
            }
        }
        "system_event" => {
            if edge.records.is_none() {
                edge_needs(door, edge, "records", diagnostics);
            }
            if edge.to.is_some() {
                // A runtime fact records a pursuit decision; it moves no state.
                edge_forbids(door, edge, "to", diagnostics);
            }
            if edge.effect.is_some() {
                edge_forbids(door, edge, "effect", diagnostics);
            }
        }
        _ => {}
    }
}

fn edge_needs(door: &str, edge: &Edge, field: &str, diagnostics: &mut Vec<Diagnostic>) {
    error(
        diagnostics,
        format!(
            "door `{door}`: edge (`{}`, `{}`) must carry `{field}`",
            edge.from, edge.input
        ),
    );
}

fn edge_forbids(door: &str, edge: &Edge, field: &str, diagnostics: &mut Vec<Diagnostic>) {
    error(
        diagnostics,
        format!(
            "door `{door}`: edge (`{}`, `{}`) must not carry `{field}`",
            edge.from, edge.input
        ),
    );
}

fn edge_kind(door: &'static str, edge: &Edge) -> EdgeKind {
    if door == "system_event" {
        return EdgeKind::Record {
            records: edge.records.clone().unwrap_or_default(),
        };
    }
    match (&edge.to, &edge.effect) {
        (Some(to), Some(effect)) => EdgeKind::External {
            to: to.clone(),
            effect: effect.clone(),
        },
        (Some(to), None) => EdgeKind::Local { to: to.clone() },
        // A proposal/fact edge missing `to` already produced an error; represent
        // it as NoEdge so the grid stays well-formed.
        (None, _) => EdgeKind::NoEdge,
    }
}

/// Every state should be reachable from `initial` by following the state-moving
/// doors (proposal, fact). An unreachable state is dead — probably a modelling
/// slip — so it is a warning, not a hard error.
fn check_reachability(topology: &Topology, diagnostics: &mut Vec<Diagnostic>) {
    let mut reachable: HashSet<&str> = HashSet::new();
    let mut frontier = vec![topology.initial.as_str()];
    reachable.insert(topology.initial.as_str());

    while let Some(state) = frontier.pop() {
        for door in &topology.doors {
            for cell in &door.cells {
                if cell.from != state {
                    continue;
                }
                let to = match &cell.kind {
                    EdgeKind::Local { to } | EdgeKind::External { to, .. } => Some(to.as_str()),
                    EdgeKind::Record { .. } | EdgeKind::NoEdge => None,
                };
                if let Some(to) = to {
                    if reachable.insert(to) {
                        frontier.push(to);
                    }
                }
            }
        }
    }

    for state in &topology.states {
        if !reachable.contains(state.as_str()) {
            warn(
                diagnostics,
                format!("state `{state}` is unreachable from `{}`", topology.initial),
            );
        }
    }
}
