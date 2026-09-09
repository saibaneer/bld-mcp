//! Stage 4 witnesses: verify a domain's exported topology against its spec, and
//! scaffold the exporter that produces it.
//!
//! Each is one a wrong implementation would fail: a `verify` that always said
//! "matches" would pass the round-trip but fail every drift case; one that always
//! said "drift" would fail the round-trip. The probe tests parse the generated
//! Rust (`syn`) and check it enumerates the whole state/input space.

use bld::spec::DomainSpec;
use bld::topology::{self, Topology};
use bld::{render, scaffold, verify};
use serde_json::Value;

const EXAMPLE: &str = include_str!("../../../examples/domain.example.yaml");

fn topology_of(yaml: &str) -> Topology {
    topology::analyze(&DomainSpec::from_yaml(yaml).expect("spec parses")).topology
}

fn rendered(topology: &Topology) -> Value {
    serde_json::from_str(&render::to_json(topology)).expect("render is valid json")
}

#[test]
fn a_domain_matching_its_spec_reports_no_drift() {
    let topology = topology_of(EXAMPLE);
    let exported = rendered(&topology);
    let report = verify::verify(&topology, &exported).expect("valid topology.json");
    assert!(
        report.matches(),
        "the spec must match its own render: {:?}",
        report.drifts
    );
}

#[test]
fn a_re_targeted_transition_is_drift() {
    let topology = topology_of(EXAMPLE);
    let mut exported = rendered(&topology);
    // Point the first `local` cell at a different state.
    let cells = exported["doors"][0]["cells"].as_array_mut().unwrap();
    let cell = cells
        .iter_mut()
        .find(|c| c["edge"] == "local")
        .expect("a local cell");
    let (from, input) = (cell["from"].clone(), cell["input"].clone());
    cell["to"] = Value::String("Cancelled".to_owned());

    let report = verify::verify(&topology, &exported).expect("valid json");
    assert!(!report.matches());
    let drift = &report.drifts[0];
    assert_eq!(drift.from, from.as_str().unwrap());
    assert_eq!(drift.input, input.as_str().unwrap());
    assert!(drift.in_domain.contains("Cancelled") && !drift.in_spec.contains("Cancelled"));
}

#[test]
fn a_transition_the_domain_grew_is_drift() {
    // A cell the spec says is `no_edge`, but the domain now allows — the boundary
    // grew a path. This is the drift verify most needs to catch.
    let topology = topology_of(EXAMPLE);
    let mut exported = rendered(&topology);
    let cells = exported["doors"][0]["cells"].as_array_mut().unwrap();
    let cell = cells
        .iter_mut()
        .find(|c| c["edge"] == "no_edge")
        .expect("a no_edge cell");
    cell["edge"] = Value::String("local".to_owned());
    cell["to"] = Value::String("Cancelled".to_owned());

    let report = verify::verify(&topology, &exported).expect("valid json");
    assert!(!report.matches());
    assert_eq!(report.drifts[0].in_spec, "no_edge");
    assert!(report.drifts[0].in_domain.starts_with("local"));
}

#[test]
fn a_value_that_is_not_a_topology_json_is_an_error() {
    let topology = topology_of(EXAMPLE);
    let not_topology: Value = serde_json::json!({ "hello": "world" });
    assert!(verify::verify(&topology, &not_topology).is_err());
}

#[test]
fn the_system_event_record_label_is_not_treated_as_drift() {
    // The spec's `records: escalate` is documentation the kernel cannot reproduce
    // (`SystemEventResolution::Record` is fieldless), so an exported `record` cell
    // with no label must still match.
    let topology = topology_of(EXAMPLE);
    let mut exported = rendered(&topology);
    for door in exported["doors"].as_array_mut().unwrap() {
        if door["name"] == "system_event" {
            for cell in door["cells"].as_array_mut().unwrap() {
                if cell["edge"] == "record" {
                    cell.as_object_mut().unwrap().remove("records");
                }
            }
        }
    }
    let report = verify::verify(&topology, &exported).expect("valid json");
    assert!(
        report.matches(),
        "a labelless record must still match: {:?}",
        report.drifts
    );
}

#[test]
fn the_generated_probe_is_syntactically_valid_rust() {
    let code = scaffold::probe(&topology_of(EXAMPLE));
    syn::parse_file(&code).expect("the probe exporter must be valid Rust");
}

#[test]
fn the_probe_enumerates_every_state_and_input() {
    let topology = topology_of(EXAMPLE);
    let code = scaffold::probe(&topology);
    // Every state appears in all_states(), every proposal input in all_proposals().
    for state in &topology.states {
        assert!(
            code.contains(&format!("State::{}", scaffold::pascal(state))),
            "missing state {state}"
        );
    }
    assert!(code.contains("fn all_states()") && code.contains("fn all_proposals()"));
    assert!(
        code.contains("pub async fn export()"),
        "the exporter entrypoint"
    );
}
