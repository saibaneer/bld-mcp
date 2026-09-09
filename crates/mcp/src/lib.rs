#![forbid(unsafe_code)]

//! A minimal Model Context Protocol server for BLD, over stdio (newline-delimited
//! JSON-RPC 2.0). It exposes the `bld` library — the SAME behaviour the CLI wraps —
//! so another team's agent can map a domain, validate and render its topology, and
//! scaffold a `BoundaryDomain`, all through tool calls.
//!
//! The protocol handling is deliberately hand-rolled and small: [`dispatch`] turns
//! one JSON-RPC request into its response (or `None` for a notification), which
//! makes the whole surface testable without a transport or a live client.

use bld::spec::DomainSpec;
use bld::topology::{self, Analysis};
use bld::{render, report, scaffold, verify};
use serde_json::{Value, json};

/// The example spec, served as a resource and used in the prompt.
const EXAMPLE_SPEC: &str = include_str!("../../../examples/domain.example.yaml");

/// A short explanation of the BLD contract + spec format, served as a resource so
/// an agent that has never seen BLD can act correctly.
const CONTRACT: &str = "\
# The BLD contract, in brief

A probabilistic component proposes; a deterministic boundary disposes. You model a
domain as a set of STATES; behaviour belongs to states. Every transition arrives
through one of three doors, by provenance:

- proposal — what someone WANTS (intent). Untrusted: a proposal is data, and an
  existing transition may still be refused by a guard, but a proposal can never
  create a transition the topology does not have.
- fact — verified external TRUTH, in your vocabulary.
- system_event — a deterministic runtime fact (a retry, a timeout). It records a
  pursuit decision and moves no state.

For every (state, input) pair you either name a transition or leave it absent. An
absent transition is a path that DOES NOT EXIST — not a rule enforced at runtime.
That is the whole security idea: illegal outcomes are unreachable by construction,
and the graph is published so an adversary already has it.

## The tools
- topology_validate(spec): totality + illegal-edge soundness on a YAML spec.
- topology_render(spec): topology.json + a Mermaid state diagram.
- scaffold_domain(spec): a Rust `impl BoundaryDomain` skeleton.
- scaffold_adversarial(spec): a test asserting every illegal transition is absent.

Author the spec in the shape of the `bld://example` resource.";

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "bld-mcp";

/// Handle one JSON-RPC request. Returns the response, or `None` for a
/// notification (a request with no `id`, e.g. `notifications/initialized`).
#[must_use]
pub fn dispatch(request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params");

    // A request without an id is a notification: act if needed, never reply.
    let id = id?;

    let outcome = match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => tools_call(params),
        "resources/list" => Ok(json!({ "resources": resources() })),
        "resources/read" => resources_read(params),
        "prompts/list" => Ok(json!({ "prompts": prompts() })),
        "prompts/get" => prompts_get(params),
        other => Err(rpc_error(-32601, &format!("method not found: {other}"))),
    };

    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}

fn rpc_error(code: i64, message: &str) -> Value {
    json!({ "code": code, "message": message })
}

fn initialize(params: Option<&Value>) -> Value {
    // Echo the client's requested protocol version when it sends one, else ours.
    let version = params
        .and_then(|p| p.get("protocolVersion"))
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {}, "resources": {}, "prompts": {} },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
    })
}

/// The one argument every tool takes: a domain spec, as YAML.
fn spec_schema(verb: &str) -> Value {
    json!({
        "type": "object",
        "properties": {
            "spec": { "type": "string", "description": format!("the domain spec to {verb}, as YAML (see the bld://example resource)") }
        },
        "required": ["spec"],
    })
}

fn tools() -> Value {
    json!([
        {
            "name": "topology_validate",
            "description": "Check a domain spec for totality (every state x input has an entry) and illegal-edge soundness. Returns a report and a VALID/INVALID verdict.",
            "inputSchema": spec_schema("validate"),
        },
        {
            "name": "topology_render",
            "description": "Render a domain spec's topology as topology.json (the published graph) plus a Mermaid state diagram.",
            "inputSchema": spec_schema("render"),
        },
        {
            "name": "scaffold_domain",
            "description": "Generate a Rust `impl BoundaryDomain` skeleton from a domain spec: the state/input/effect enums and the three doors, with guards left as TODO and every illegal transition absent.",
            "inputSchema": spec_schema("scaffold"),
        },
        {
            "name": "scaffold_adversarial",
            "description": "Generate a Rust test that asserts every illegal (state, proposal) is Undefined — the boundary's structure, witnessed.",
            "inputSchema": spec_schema("scaffold from"),
        },
        {
            "name": "scaffold_probe",
            "description": "Generate a Rust exporter that emits the domain's real topology.json, for topology_verify to check against the spec.",
            "inputSchema": spec_schema("scaffold a probe for"),
        },
        {
            "name": "topology_verify",
            "description": "Check a domain's exported topology.json against its spec and report any drift (a transition the code grew, lost, or re-targeted).",
            "inputSchema": json!({
                "type": "object",
                "properties": {
                    "spec": { "type": "string", "description": "the domain spec, as YAML" },
                    "topology": { "type": "string", "description": "the domain's exported topology.json (from `scaffold probe`)" }
                },
                "required": ["spec", "topology"],
            }),
        },
    ])
}

fn tools_call(params: Option<&Value>) -> Result<Value, Value> {
    let params = params.ok_or_else(|| rpc_error(-32602, "missing params"))?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32602, "missing tool name"))?;
    let args = params.get("arguments");
    let spec = args
        .and_then(|a| a.get("spec"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let topology_arg = args
        .and_then(|a| a.get("topology"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let outcome = match name {
        "topology_validate" => run(spec, |a| {
            format!("{}\n{}", report::human(a), report::verdict(a))
        }),
        "topology_render" => run(spec, |a| {
            if a.is_sound() {
                format!(
                    "topology.json:\n{}\n\ntopology.mmd:\n{}",
                    render::to_json(&a.topology),
                    render::to_mermaid(&a.topology)
                )
            } else {
                format!(
                    "{}\n{}\n\n(refusing to render an unsound topology)",
                    report::human(a),
                    report::verdict(a)
                )
            }
        }),
        "scaffold_domain" => run(spec, |a| scaffold_or_refuse(a, scaffold::domain)),
        "scaffold_adversarial" => run(spec, |a| scaffold_or_refuse(a, scaffold::adversarial)),
        "scaffold_probe" => run(spec, |a| scaffold_or_refuse(a, scaffold::probe)),
        "topology_verify" => run_verify(spec, topology_arg),
        other => return Err(rpc_error(-32602, &format!("unknown tool: {other}"))),
    };

    Ok(match outcome {
        Ok(text) => json!({ "content": [{ "type": "text", "text": text }], "isError": false }),
        Err(text) => json!({ "content": [{ "type": "text", "text": text }], "isError": true }),
    })
}

/// Parse the spec, then hand a sound-or-not analysis to `render`. A parse failure
/// is the only tool-level error (`Err`); an unsound-but-parsed spec is a normal
/// result whose text explains why.
fn run(spec: &str, render_ok: impl Fn(&Analysis) -> String) -> Result<String, String> {
    match DomainSpec::from_yaml(spec) {
        Ok(parsed) => Ok(render_ok(&topology::analyze(&parsed))),
        Err(error) => Err(format!("the spec did not parse: {error}")),
    }
}

fn scaffold_or_refuse(
    analysis: &Analysis,
    generate: impl Fn(&bld::topology::Topology) -> String,
) -> String {
    if analysis.is_sound() {
        generate(&analysis.topology)
    } else {
        format!(
            "{}\n\n(refusing to scaffold from an unsound topology — fix the errors first)",
            report::verdict(analysis)
        )
    }
}

/// Verify an exported `topology.json` against a spec. A parse failure of either is
/// the tool error; a sound spec plus a valid `topology.json` yields the drift report.
fn run_verify(spec: &str, exported: &str) -> Result<String, String> {
    let parsed =
        DomainSpec::from_yaml(spec).map_err(|error| format!("the spec did not parse: {error}"))?;
    let analysis = topology::analyze(&parsed);
    if !analysis.is_sound() {
        return Ok(format!(
            "{}\n\n(fix the spec's errors before verifying a domain against it)",
            report::verdict(&analysis)
        ));
    }
    let value: Value = serde_json::from_str(exported)
        .map_err(|error| format!("the exported topology.json did not parse: {error}"))?;
    match verify::verify(&analysis.topology, &value) {
        Ok(result) => Ok(verify::report(&result)),
        Err(why) => Err(format!("the exported value is not a topology.json: {why}")),
    }
}

fn resources() -> Value {
    json!([
        {
            "uri": "bld://example",
            "name": "Example domain spec",
            "description": "A commented, domain-neutral YAML spec to author yours after.",
            "mimeType": "text/yaml",
        },
        {
            "uri": "bld://contract",
            "name": "The BLD contract",
            "description": "How BLD works and how to shape a domain spec.",
            "mimeType": "text/markdown",
        },
    ])
}

fn resources_read(params: Option<&Value>) -> Result<Value, Value> {
    let uri = params
        .and_then(|p| p.get("uri"))
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32602, "missing resource uri"))?;
    let (text, mime) = match uri {
        "bld://example" => (EXAMPLE_SPEC, "text/yaml"),
        "bld://contract" => (CONTRACT, "text/markdown"),
        other => return Err(rpc_error(-32602, &format!("unknown resource: {other}"))),
    };
    Ok(json!({ "contents": [{ "uri": uri, "mimeType": mime, "text": text }] }))
}

fn prompts() -> Value {
    json!([
        {
            "name": "map_domain",
            "description": "Interview the user about their domain and produce a BLD spec.",
            "arguments": [
                { "name": "description", "description": "a plain-language description of the domain, if you have one", "required": false }
            ],
        },
    ])
}

fn prompts_get(params: Option<&Value>) -> Result<Value, Value> {
    let name = params
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32602, "missing prompt name"))?;
    if name != "map_domain" {
        return Err(rpc_error(-32602, &format!("unknown prompt: {name}")));
    }
    let description = params
        .and_then(|p| p.get("arguments"))
        .and_then(|a| a.get("description"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let text = format!(
        "You are helping map a domain onto Boundary-Led Development. Read the \
         `bld://contract` resource for the model and `bld://example` for the spec shape.\n\n\
         Interview the user to establish, in order:\n\
         1. the STATES the domain can be in, and the one it starts in;\n\
         2. for each state, what a person can DO (proposal inputs) and where each leads \
         — mark the ones that reach out to the world as `external` with an effect;\n\
         3. what the WORLD later confirms (fact inputs) and where those lead;\n\
         4. any runtime facts (timeouts/retries) as system_event inputs.\n\n\
         Produce a YAML spec in the shape of `bld://example`. Then call \
         `topology_validate` on it, fix anything it reports, and offer `scaffold_domain`.\n\n\
         {}",
        if description.is_empty() {
            "The user has not described the domain yet — ask them to, in one or two sentences."
                .to_owned()
        } else {
            format!("The user describes their domain as:\n{description}")
        }
    );

    Ok(json!({
        "description": "Map a domain onto BLD and produce a validated spec.",
        "messages": [
            { "role": "user", "content": { "type": "text", "text": text } }
        ],
    }))
}
