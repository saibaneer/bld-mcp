#![forbid(unsafe_code)]

//! The `bld` CLI. Thin dispatch over the [`bld`] library; the MCP server will
//! wrap this same binary, so all behaviour lives in the library, not here.

use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use bld::spec::DomainSpec;
use bld::topology::{self, Analysis, EdgeKind, Severity};
use bld::{render, scaffold};

const USAGE: &str = "\
bld — map a domain, derive and validate its topology, scaffold a BoundaryDomain

USAGE:
    bld <command> [args]

COMMANDS:
    topology validate <spec.yaml>     Check a domain spec for totality and
                                      illegal-edge soundness
    topology render   <spec.yaml>     Write topology.json + topology.mmd (Mermaid)
    scaffold domain   <spec.yaml>     Generate an impl BoundaryDomain skeleton
                                      (Stage 2)
    scaffold adversarial <spec.yaml>  Generate the topology-adversary + hostile
                                      proposer harness (Stage 2)
    help                              Show this help

See docs/design.md for the design and examples/domain.example.yaml for a
commented, domain-neutral template.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parts: Vec<&str> = args.iter().map(String::as_str).collect();

    match parts.as_slice() {
        [] | ["help" | "-h" | "--help"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        ["topology", "validate", spec] => cmd_validate(Path::new(spec)),
        ["topology", "render", spec] => cmd_render(Path::new(spec)),
        ["scaffold", "domain", spec] => cmd_scaffold(Path::new(spec), Kind::Domain),
        ["scaffold", "adversarial", spec] => cmd_scaffold(Path::new(spec), Kind::Adversarial),
        _ => {
            eprintln!("bld: unrecognized command: {}\n", args.join(" "));
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Load and parse a spec, mapping a parse failure to a printed error + exit 2.
fn load(path: &Path) -> Result<DomainSpec, ExitCode> {
    DomainSpec::from_path(path).map_err(|error| {
        eprintln!("bld: {error}");
        ExitCode::from(2)
    })
}

fn cmd_validate(path: &Path) -> ExitCode {
    let spec = match load(path) {
        Ok(spec) => spec,
        Err(code) => return code,
    };
    let analysis = topology::analyze(&spec);
    print!("{}", report(&analysis));

    if analysis.is_sound() {
        println!(
            "\nVALID — the topology is total ({} no_edge cells made explicit) and sound.",
            analysis.no_edge_count()
        );
        ExitCode::SUCCESS
    } else {
        println!(
            "\nINVALID — {} error(s). The topology is not sound.",
            analysis.error_count()
        );
        ExitCode::from(1)
    }
}

fn cmd_render(path: &Path) -> ExitCode {
    let spec = match load(path) {
        Ok(spec) => spec,
        Err(code) => return code,
    };
    let analysis = topology::analyze(&spec);
    if !analysis.is_sound() {
        print!("{}", report(&analysis));
        eprintln!(
            "\nbld: refusing to render an unsound topology ({} error(s)). \
             Fix them (`bld topology validate {}`) first.",
            analysis.error_count(),
            path.display()
        );
        return ExitCode::from(1);
    }

    let json = render::to_json(&analysis.topology);
    let mermaid = render::to_mermaid(&analysis.topology);
    let json_path = Path::new("topology.json");
    let mmd_path = Path::new("topology.mmd");

    for (target, contents) in [(json_path, &json), (mmd_path, &mermaid)] {
        if let Err(error) = std::fs::write(target, contents) {
            eprintln!("bld: cannot write {}: {error}", target.display());
            return ExitCode::from(2);
        }
    }

    println!("wrote {} and {}", json_path.display(), mmd_path.display());
    if analysis.warning_count() > 0 {
        println!(
            "({} warning(s) — see `bld topology validate`)",
            analysis.warning_count()
        );
    }
    println!("\n{mermaid}");
    ExitCode::SUCCESS
}

/// A human-readable report of a spec's per-door shape and every diagnostic.
fn report(analysis: &Analysis) -> String {
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

    let warnings: Vec<&str> = analysis
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .map(|d| d.message.as_str())
        .collect();
    if !warnings.is_empty() {
        out.push_str("\nwarnings:\n");
        for message in warnings {
            let _ = writeln!(out, "  - {message}");
        }
    }

    let errors: Vec<&str> = analysis
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.as_str())
        .collect();
    if !errors.is_empty() {
        out.push_str("\nerrors:\n");
        for message in errors {
            let _ = writeln!(out, "  - {message}");
        }
    }
    out
}

/// Which artifact `scaffold` emits.
#[derive(Clone, Copy)]
enum Kind {
    Domain,
    Adversarial,
}

fn cmd_scaffold(path: &Path, kind: Kind) -> ExitCode {
    let spec = match load(path) {
        Ok(spec) => spec,
        Err(code) => return code,
    };
    let analysis = topology::analyze(&spec);
    if !analysis.is_sound() {
        print!("{}", report(&analysis));
        eprintln!(
            "\nbld: refusing to scaffold from an unsound topology ({} error(s)). \
             Fix them (`bld topology validate {}`) first.",
            analysis.error_count(),
            path.display()
        );
        return ExitCode::from(1);
    }

    let stem = scaffold::snake(&analysis.topology.domain);
    let (contents, out_name) = match kind {
        Kind::Domain => (scaffold::domain(&analysis.topology), format!("{stem}.rs")),
        Kind::Adversarial => (
            scaffold::adversarial(&analysis.topology),
            format!("{stem}_adversarial.rs"),
        ),
    };
    let out_path = Path::new(&out_name);
    if let Err(error) = std::fs::write(out_path, &contents) {
        eprintln!("bld: cannot write {}: {error}", out_path.display());
        return ExitCode::from(2);
    }
    println!("wrote {}", out_path.display());
    println!("(a skeleton — fill the TODOs; compilation against bld-kernel is a Stage-4 check)");
    ExitCode::SUCCESS
}
