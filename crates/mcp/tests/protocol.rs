//! Protocol witnesses: drive `dispatch` with JSON-RPC requests and assert the
//! responses. Each is one a wrong dispatch would fail — a tool that ignored its
//! spec, a validator wired backwards, a missing resource, a notification that
//! wrongly replied.

use bld_mcp::dispatch;
use serde_json::{Value, json};

const EXAMPLE: &str = include_str!("../../../examples/domain.example.yaml");

// The `json!` macro consumes `params`, but clippy can't see through it.
#[allow(clippy::needless_pass_by_value)]
fn req(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
}

/// Call a tool and return the text of its single content block.
fn tool_text(name: &str, spec: &str) -> String {
    let response = dispatch(&req(
        "tools/call",
        json!({ "name": name, "arguments": { "spec": spec } }),
    ))
    .expect("a request with an id gets a response");
    response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content")
        .to_owned()
}

#[test]
fn initialize_announces_the_server_and_capabilities() {
    let response = dispatch(&req(
        "initialize",
        json!({ "protocolVersion": "2024-11-05" }),
    ))
    .unwrap();
    let result = &response["result"];
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["capabilities"]["resources"].is_object());
    assert!(result["capabilities"]["prompts"].is_object());
    assert_eq!(result["serverInfo"]["name"], "bld-mcp");
}

#[test]
fn tools_list_advertises_the_four_tools_each_requiring_a_spec() {
    let response = dispatch(&req("tools/list", json!({}))).unwrap();
    let tools = response["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(
        names,
        vec![
            "topology_validate",
            "topology_render",
            "scaffold_domain",
            "scaffold_adversarial"
        ]
    );
    for tool in tools {
        assert_eq!(
            tool["inputSchema"]["required"],
            json!(["spec"]),
            "{} must require a spec",
            tool["name"]
        );
    }
}

#[test]
fn validate_reports_valid_for_the_example() {
    let text = tool_text("topology_validate", EXAMPLE);
    assert!(text.contains("VALID"), "{text}");
    assert!(!text.contains("INVALID"), "the example is sound");
}

#[test]
fn validate_reports_invalid_for_an_unsound_spec() {
    // An edge to an undeclared state — the validator must catch it, not pass it.
    let bad = "domain: d\ninitial: A\nstates: [A]\n\
               proposal:\n  inputs: [go]\n  edges:\n    - { from: A, input: go, to: Nowhere }\n";
    let response = dispatch(&req(
        "tools/call",
        json!({ "name": "topology_validate", "arguments": { "spec": bad } }),
    ))
    .unwrap();
    assert_eq!(
        response["result"]["isError"], false,
        "an unsound spec is a valid result"
    );
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("INVALID") && text.contains("Nowhere"),
        "{text}"
    );
}

#[test]
fn an_unparseable_spec_is_a_tool_error() {
    let response = dispatch(&req(
        "tools/call",
        json!({ "name": "topology_validate", "arguments": { "spec": "not: a: spec" } }),
    ))
    .unwrap();
    assert_eq!(
        response["result"]["isError"], true,
        "a non-spec is a tool error"
    );
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("did not parse")
    );
}

#[test]
fn render_returns_the_graph_and_the_diagram() {
    let text = tool_text("topology_render", EXAMPLE);
    assert!(text.contains("stateDiagram-v2"), "a Mermaid diagram");
    assert!(
        text.contains("\"edge\": \"no_edge\""),
        "topology.json with no_edge cells"
    );
}

#[test]
fn scaffold_domain_returns_a_boundary_domain_impl() {
    let text = tool_text("scaffold_domain", EXAMPLE);
    assert!(
        text.contains("impl BoundaryDomain for ApprovalWorkflow"),
        "{text}"
    );
    assert!(text.contains("_ => Resolution::Undefined"));
}

#[test]
fn scaffold_refuses_an_unsound_spec() {
    let bad = "domain: d\ninitial: A\nstates: [A]\n\
               proposal:\n  inputs: [go]\n  edges:\n    - { from: A, input: go, to: Nowhere }\n";
    let text = tool_text("scaffold_domain", bad);
    assert!(text.contains("refusing to scaffold"), "{text}");
    assert!(
        !text.contains("impl BoundaryDomain"),
        "no code from an unsound spec"
    );
}

#[test]
fn resources_list_and_read_the_example_and_contract() {
    let list = dispatch(&req("resources/list", json!({}))).unwrap();
    let uris: Vec<&str> = list["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["uri"].as_str())
        .collect();
    assert!(uris.contains(&"bld://example") && uris.contains(&"bld://contract"));

    let read = dispatch(&req("resources/read", json!({ "uri": "bld://example" }))).unwrap();
    let text = read["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("domain: approval-workflow"),
        "the example spec is served"
    );
}

#[test]
fn map_domain_prompt_guides_toward_a_validated_spec() {
    let response = dispatch(&req(
        "prompts/get",
        json!({ "name": "map_domain", "arguments": {} }),
    ))
    .unwrap();
    let messages = response["result"]["messages"].as_array().expect("messages");
    assert!(!messages.is_empty());
    let text = messages[0]["content"]["text"].as_str().unwrap();
    assert!(
        text.contains("topology_validate"),
        "the prompt points at validation"
    );
}

#[test]
fn a_notification_gets_no_response() {
    // No `id` -> a notification. It must not produce a reply.
    let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(dispatch(&note).is_none());
}

#[test]
fn an_unknown_method_is_a_jsonrpc_error() {
    let response = dispatch(&req("does/not/exist", json!({}))).unwrap();
    assert_eq!(response["error"]["code"], -32601);
    assert!(response.get("result").is_none());
}

#[test]
fn an_unknown_tool_is_an_error() {
    let response = dispatch(&req(
        "tools/call",
        json!({ "name": "nope", "arguments": { "spec": "" } }),
    ))
    .unwrap();
    assert_eq!(response["error"]["code"], -32602);
}
