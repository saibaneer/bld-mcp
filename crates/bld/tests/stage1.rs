//! Stage 1 witnesses: parse → derive the full grid → validate → render.
//!
//! Each test is one a *wrong* implementation would fail. The soundness tests feed
//! a spec whose ONLY defect is the one under test and assert the specific
//! diagnostic, so a validator that skipped that check would let the spec pass and
//! break the test. The totality test asserts the grid is complete by count, so a
//! derivation that forgot to fill `no_edge` cells would fail it.

use bld::render;
use bld::spec::DomainSpec;
use bld::topology::{self, Analysis, EdgeKind};

fn analyze(yaml: &str) -> Analysis {
    let spec = DomainSpec::from_yaml(yaml).expect("spec parses");
    topology::analyze(&spec)
}

/// True when some error diagnostic contains `needle`.
fn has_error(analysis: &Analysis, needle: &str) -> bool {
    analysis
        .diagnostics
        .iter()
        .filter(|d| matches!(d.severity, bld::topology::Severity::Error))
        .any(|d| d.message.contains(needle))
}

const EXAMPLE: &str = include_str!("../../../examples/domain.example.yaml");

#[test]
fn the_shipped_example_is_sound() {
    let analysis = analyze(EXAMPLE);
    assert!(
        analysis.is_sound(),
        "the example must validate clean, got: {:?}",
        analysis.diagnostics
    );
    // Its shape, pinned so an accidental edit to the example is caught here.
    assert_eq!(analysis.topology.states.len(), 8);
    assert_eq!(
        analysis.topology.doors.len(),
        3,
        "proposal + fact + system_event"
    );
}

#[test]
fn every_state_input_pair_becomes_a_cell() {
    // Totality by construction: each door's grid is exactly states x inputs, with
    // the unstated pairs filled as no_edge. A derivation that only emitted the
    // authored edges would fail this.
    let analysis = analyze(EXAMPLE);
    let states = analysis.topology.states.len();
    for door in &analysis.topology.doors {
        assert_eq!(
            door.cells.len(),
            states * door.inputs.len(),
            "door `{}` grid must be total",
            door.name
        );
    }
    assert!(
        analysis.no_edge_count() > 0,
        "a real domain leaves most pairs as no_edge — the absence made explicit"
    );
}

#[test]
fn an_edge_to_an_undeclared_state_is_an_error() {
    let analysis = analyze(
        "domain: d\ninitial: A\nstates: [A, B]\n\
         proposal:\n  inputs: [go]\n  edges:\n    - { from: A, input: go, to: Nowhere }\n",
    );
    assert!(!analysis.is_sound());
    assert!(
        has_error(&analysis, "Nowhere"),
        "must name the undeclared target: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn two_edges_for_one_cell_is_an_error() {
    let analysis = analyze(
        "domain: d\ninitial: A\nstates: [A, B]\n\
         proposal:\n  inputs: [go]\n  edges:\n    \
         - { from: A, input: go, to: B }\n    - { from: A, input: go, to: A }\n",
    );
    assert!(!analysis.is_sound());
    assert!(
        has_error(&analysis, "only one answer"),
        "a cell with two answers must be refused: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn an_input_the_door_does_not_accept_is_an_error() {
    let analysis = analyze(
        "domain: d\ninitial: A\nstates: [A, B]\n\
         proposal:\n  inputs: [go]\n  edges:\n    - { from: A, input: leap, to: B }\n",
    );
    assert!(!analysis.is_sound());
    assert!(has_error(&analysis, "not one of the door's inputs"));
}

#[test]
fn a_proposal_edge_without_a_target_is_an_error() {
    let analysis = analyze(
        "domain: d\ninitial: A\nstates: [A, B]\n\
         proposal:\n  inputs: [go]\n  edges:\n    - { from: A, input: go }\n",
    );
    assert!(!analysis.is_sound());
    assert!(has_error(&analysis, "must carry `to`"));
}

#[test]
fn a_system_event_edge_that_moves_state_is_an_error() {
    // A runtime fact records a pursuit decision; it must not move state. An edge
    // with `to` and no `records` is two distinct errors.
    let analysis = analyze(
        "domain: d\ninitial: A\nstates: [A, B]\n\
         proposal:\n  inputs: [go]\n  edges:\n    - { from: A, input: go, to: B }\n\
         system_event:\n  inputs: [tick]\n  edges:\n    - { from: A, input: tick, to: B }\n",
    );
    assert!(!analysis.is_sound());
    assert!(has_error(&analysis, "must not carry `to`"));
    assert!(has_error(&analysis, "must carry `records`"));
}

#[test]
fn an_unreachable_state_is_a_warning_not_an_error() {
    // `Island` cannot be reached from `A`. The domain is still SOUND (an error
    // would be too strong — you might be mid-modelling) but it is warned about.
    let analysis = analyze(
        "domain: d\ninitial: A\nstates: [A, B, Island]\n\
         proposal:\n  inputs: [go]\n  edges:\n    - { from: A, input: go, to: B }\n",
    );
    assert!(
        analysis.is_sound(),
        "unreachability is a warning, not an error"
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Island") && d.message.contains("unreachable")),
        "the unreachable state must be surfaced: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn render_json_is_total_and_marks_edge_kinds() {
    let analysis = analyze(EXAMPLE);
    let json: serde_json::Value =
        serde_json::from_str(&render::to_json(&analysis.topology)).expect("valid json");

    let doors = json["doors"].as_array().expect("doors array");
    let total_cells: usize = doors
        .iter()
        .map(|d| d["cells"].as_array().map_or(0, Vec::len))
        .sum();
    let expected: usize = analysis
        .topology
        .doors
        .iter()
        .map(|d| analysis.topology.states.len() * d.inputs.len())
        .sum();
    assert_eq!(total_cells, expected, "the rendered grid must be total");

    // The example's Approved --Commit--> Committing edge is external, carrying the
    // effect PerformCommit. A render that dropped the effect would fail this.
    let external = doors
        .iter()
        .flat_map(|d| d["cells"].as_array().cloned().unwrap_or_default())
        .find(|c| c["edge"] == "external")
        .expect("an external edge exists");
    assert_eq!(external["effect"], "PerformCommit");
}

#[test]
fn a_system_event_effect_is_kept_out_of_the_state_diagram() {
    // system_event records a decision and moves no state, so it must NOT appear as
    // a transition in the Mermaid state diagram.
    let analysis = analyze(EXAMPLE);
    let mermaid = render::to_mermaid(&analysis.topology);
    assert!(
        mermaid.contains("Draft --> Submitted"),
        "state edges are drawn"
    );
    assert!(
        !mermaid.contains("CommitTimedOut"),
        "a system_event input must not be a transition in the diagram"
    );
}

#[test]
fn a_record_cell_survives_derivation() {
    // The example's one system_event edge records `escalate`; the grid must carry
    // it as a Record cell (not lost, not turned into a state move).
    let analysis = analyze(EXAMPLE);
    let has_record = analysis
        .topology
        .doors
        .iter()
        .flat_map(|d| &d.cells)
        .any(|c| matches!(&c.kind, EdgeKind::Record { records } if records == "escalate"));
    assert!(has_record, "the system_event record cell must be present");
}

#[test]
fn malformed_yaml_is_a_clean_error_not_a_panic() {
    let err = DomainSpec::from_yaml("this: is: not: a: spec").unwrap_err();
    assert!(matches!(err, bld::spec::ParseError::Yaml(_)));
}
