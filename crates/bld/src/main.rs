#![forbid(unsafe_code)]

//! The `bld` CLI. Thin dispatch over the [`bld`] library; the MCP server will
//! wrap this same binary, so all behaviour lives in the library, not here.

use std::path::Path;
use std::process::ExitCode;

use bld::spec::DomainSpec;
use bld::topology;
use bld::{render, report, scaffold, verify};

const USAGE: &str = "\
bld — map a domain, derive and validate its topology, scaffold a BoundaryDomain

USAGE:
    bld <command> [args]

COMMANDS:
    topology validate <spec.yaml>            Check a spec for totality + soundness
    topology render   <spec.yaml>            Write topology.json + topology.mmd
    topology verify   <spec.yaml> <t.json>   Check an exported topology.json against
                                             the spec — catch drift
    scaffold domain      <spec.yaml>         An impl BoundaryDomain skeleton
    scaffold adversarial <spec.yaml>         A harness asserting illegal edges absent
    scaffold probe       <spec.yaml>         An exporter that emits the domain's real
                                             topology.json for `topology verify`
    help                                     Show this help

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
        ["topology", "verify", spec, exported] => cmd_verify(Path::new(spec), Path::new(exported)),
        ["scaffold", "domain", spec] => cmd_scaffold(Path::new(spec), Kind::Domain),
        ["scaffold", "adversarial", spec] => cmd_scaffold(Path::new(spec), Kind::Adversarial),
        ["scaffold", "probe", spec] => cmd_scaffold(Path::new(spec), Kind::Probe),
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
    print!("{}", report::human(&analysis));
    println!("\n{}", report::verdict(&analysis));
    if analysis.is_sound() {
        ExitCode::SUCCESS
    } else {
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
        print!("{}", report::human(&analysis));
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

/// Which artifact `scaffold` emits.
#[derive(Clone, Copy)]
enum Kind {
    Domain,
    Adversarial,
    Probe,
}

fn cmd_verify(spec_path: &Path, exported_path: &Path) -> ExitCode {
    let spec = match load(spec_path) {
        Ok(spec) => spec,
        Err(code) => return code,
    };
    let analysis = topology::analyze(&spec);
    if !analysis.is_sound() {
        print!("{}", report::human(&analysis));
        eprintln!("\nbld: fix the spec's errors before verifying a domain against it.");
        return ExitCode::from(1);
    }
    let text = match std::fs::read_to_string(exported_path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("bld: cannot read {}: {error}", exported_path.display());
            return ExitCode::from(2);
        }
    };
    let exported: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            eprintln!(
                "bld: {} is not valid JSON: {error}",
                exported_path.display()
            );
            return ExitCode::from(2);
        }
    };
    match verify::verify(&analysis.topology, &exported) {
        Ok(result) => {
            print!("{}", verify::report(&result));
            if result.matches() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(why) => {
            eprintln!(
                "bld: {} is not a topology.json: {why}",
                exported_path.display()
            );
            ExitCode::from(2)
        }
    }
}

fn cmd_scaffold(path: &Path, kind: Kind) -> ExitCode {
    let spec = match load(path) {
        Ok(spec) => spec,
        Err(code) => return code,
    };
    let analysis = topology::analyze(&spec);
    if !analysis.is_sound() {
        print!("{}", report::human(&analysis));
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
        Kind::Probe => (
            scaffold::probe(&analysis.topology),
            format!("{stem}_probe.rs"),
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
