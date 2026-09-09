//! Stage 2 witnesses: scaffold a domain skeleton + adversarial harness.
//!
//! Full compilation against `bld-kernel` is a Stage-4 check. What Stage 2 can
//! honestly witness on its own: the generated text is *syntactically valid Rust*
//! (`syn::parse_file`), and its structure matches the topology it came from —
//! every state/input is declared, every authored edge is a `Ready` arm, and the
//! adversarial harness enumerates exactly the `no_edge` pairs. A generator that
//! dropped a state, an edge, or a `no_edge` fails these.

use bld::scaffold;
use bld::spec::DomainSpec;
use bld::topology::{self, EdgeKind, Topology};

const EXAMPLE: &str = include_str!("../../../examples/domain.example.yaml");

fn topology_of(yaml: &str) -> Topology {
    let spec = DomainSpec::from_yaml(yaml).expect("spec parses");
    let analysis = topology::analyze(&spec);
    assert!(
        analysis.is_sound(),
        "fixture must be sound: {:?}",
        analysis.diagnostics
    );
    analysis.topology
}

/// The variant idents of a named enum in generated Rust — via the real AST, not
/// string matching, so a malformed enum fails at the parse.
fn enum_variants(code: &str, name: &str) -> Vec<String> {
    let file = syn::parse_file(code).expect("generated code must be valid Rust");
    file.items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Enum(e) if e.ident == name => {
                Some(e.variants.iter().map(|v| v.ident.to_string()).collect())
            }
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn generated_domain_is_syntactically_valid_rust() {
    let code = scaffold::domain(&topology_of(EXAMPLE));
    syn::parse_file(&code).expect("the domain skeleton must be valid Rust");
}

#[test]
fn generated_adversarial_is_syntactically_valid_rust() {
    let code = scaffold::adversarial(&topology_of(EXAMPLE));
    syn::parse_file(&code).expect("the adversarial harness must be valid Rust");
}

#[test]
fn the_domain_declares_every_state_and_input() {
    let topology = topology_of(EXAMPLE);
    let code = scaffold::domain(&topology);

    let states = enum_variants(&code, "State");
    let expected: Vec<String> = topology
        .states
        .iter()
        .map(|s| scaffold::pascal(s))
        .collect();
    assert_eq!(states, expected, "every state must be a State variant");

    let proposal_inputs = &topology
        .doors
        .iter()
        .find(|d| d.name == "proposal")
        .unwrap()
        .inputs;
    let generated: Vec<String> = enum_variants(&code, "Proposal");
    let expected_inputs: Vec<String> = proposal_inputs
        .iter()
        .map(|i| scaffold::pascal(i))
        .collect();
    assert_eq!(
        generated, expected_inputs,
        "every proposal input must be a variant"
    );
}

#[test]
fn an_external_edge_becomes_an_effect_dispatching_arm() {
    // Approved --Commit--> Committing is external; the arm must carry the effect.
    let code = scaffold::domain(&topology_of(EXAMPLE));
    assert!(
        code.contains("TransitionPlan::ExternalEffect")
            && code.contains("effect: Effect::PerformCommit"),
        "the external edge must dispatch its effect"
    );
    // And the catch-all absence must be present, or illegal transitions would not
    // be Undefined.
    assert!(code.contains("_ => Resolution::Undefined"));
}

#[test]
fn the_adversarial_harness_enumerates_exactly_the_no_edges() {
    let topology = topology_of(EXAMPLE);
    let proposal = topology
        .doors
        .iter()
        .find(|d| d.name == "proposal")
        .unwrap();
    let no_edges = proposal
        .cells
        .iter()
        .filter(|c| matches!(c.kind, EdgeKind::NoEdge))
        .count();
    assert!(no_edges > 0);

    let code = scaffold::adversarial(&topology);
    // Every illegal pair is written as `(State::X, Proposal::Y),`; that token only
    // occurs in the illegal_proposals list.
    let listed = code.matches("(State::").count();
    assert_eq!(
        listed, no_edges,
        "the harness must list exactly the proposal no_edge pairs"
    );
}

#[test]
fn an_absent_door_yields_an_empty_enum_and_an_undefined_body() {
    // A spec with no system_event door still needs the associated type + method.
    let code = scaffold::domain(&topology_of(
        "domain: tiny\ninitial: A\nstates: [A, B]\n\
         proposal:\n  inputs: [go]\n  edges:\n    - { from: A, input: go, to: B }\n",
    ));
    syn::parse_file(&code).expect("valid rust even with absent doors");
    assert!(
        code.contains("pub enum SystemEvent {}"),
        "absent door -> empty enum"
    );
    assert!(
        code.contains("pub enum ProviderFact {}"),
        "absent fact door -> empty enum"
    );
    assert!(
        code.contains("SystemEventResolution::Undefined"),
        "an absent door resolves everything Undefined"
    );
}

#[test]
fn names_are_normalized_to_pascal_case_variants() {
    // A spec using snake_case input names must still yield valid PascalCase variants.
    let code = scaffold::domain(&topology_of(
        "domain: casing\ninitial: start\nstates: [start, done]\n\
         proposal:\n  inputs: [move_on]\n  edges:\n    - { from: start, input: move_on, to: done }\n",
    ));
    let states = enum_variants(&code, "State");
    assert_eq!(states, vec!["Start", "Done"]);
    let inputs = enum_variants(&code, "Proposal");
    assert_eq!(inputs, vec!["MoveOn"]);
}
