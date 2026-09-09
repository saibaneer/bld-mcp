#![forbid(unsafe_code)]

//! The BLD CLI — the source of truth the MCP server will wrap.
//!
//! BLD ("Boundary-Led Development") is the discipline proved by the town-hall
//! reference: a deterministic core over which a probabilistic component
//! proposes and cannot bypass; behaviour belongs to states; the legal/illegal
//! transition graph (the *topology*) is published so an adversary already has
//! it. This tool lets a team map their own domain as a small YAML spec, derive
//! and validate that topology, and scaffold a `BoundaryDomain` skeleton — so the
//! discipline is replicable without re-deriving it from scratch.
//!
//! This first commit is a buildable STUB: it declares the command surface from
//! the design note (`docs/design.md`) and reports each command as not-yet-built.
//! Stage 1 fills in `topology validate` / `topology render`.

use std::process::ExitCode;

const USAGE: &str = "\
bld — map a domain, derive and validate its topology, scaffold a BoundaryDomain

USAGE:
    bld <command> [args]

COMMANDS:
    topology validate <spec.yaml>     Check a domain spec for totality and
                                      illegal-edge soundness (Stage 1)
    topology render   <spec.yaml>     Emit topology.json + a diagram (Stage 1)
    scaffold domain   <spec.yaml>     Generate an impl BoundaryDomain skeleton
                                      (Stage 2)
    scaffold adversarial <spec.yaml>  Generate the topology-adversary + hostile
                                      proposer harness (Stage 2)
    help                              Show this help

See docs/design.md for the full design and the YAML spec shape, and
examples/domain.example.yaml for a commented, domain-neutral template.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parts: Vec<&str> = args.iter().map(String::as_str).collect();

    match parts.as_slice() {
        [] | ["help" | "-h" | "--help"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        ["topology", "validate", _spec] => not_yet("topology validate", 1),
        ["topology", "render", _spec] => not_yet("topology render", 1),
        ["scaffold", "domain", _spec] => not_yet("scaffold domain", 2),
        ["scaffold", "adversarial", _spec] => not_yet("scaffold adversarial", 2),
        _ => {
            eprintln!("bld: unrecognized command: {}\n", args.join(" "));
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// A planned-but-unbuilt command: name the stage it belongs to and fail loudly
/// (exit 3), so a caller never mistakes a stub for success.
fn not_yet(command: &str, stage: u8) -> ExitCode {
    eprintln!("bld: `{command}` is not implemented yet (Stage {stage}). See docs/design.md.");
    ExitCode::from(3)
}
