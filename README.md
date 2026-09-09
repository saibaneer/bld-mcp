# bld — Boundary-Led Development, for your domain

> A probabilistic component proposes; a deterministic boundary disposes.

**Boundary-Led Development (BLD)** is a discipline for building systems where an
untrusted, probabilistic component (an LLM agent, a remote client, a model) may
*propose* actions but can never *bypass* the deterministic core that decides what
actually happens. Behaviour belongs to states; the legal and illegal transitions
— the **topology** — are published so an adversary already has the map; and an
adversarial test suite proves the boundary holds regardless of the proposer.

The pattern was proved end-to-end in a reference implementation (a town-hall
booking service). **This repository turns the reusable half into a tool** so
another team can replicate the discipline in *their* domain — loan approval,
content moderation, order fulfilment, escrow — without re-deriving it.

At minimum: **map your domain and build out its topology.**

## What it does

You describe your domain as a small YAML spec — its states, and for each state
which inputs are legal and where they lead. `bld` then:

- **validates** the topology — every `(state, input)` pair must have an entry, so
  no input sequence can reach a transition nobody specified (*totality*), and an
  illegal transition is *absent* rather than guarded (a path that does not exist,
  not a rule that can be gotten wrong);
- **renders** the graph (`topology.json` + a diagram) so it is reviewable;
- **scaffolds** a Rust `impl BoundaryDomain` skeleton and an adversarial harness
  from the same spec.

The `bld` CLI is the source of truth; an MCP server (later) wraps it so a coding
agent can drive the whole flow. Rust-first: the scaffolding targets the
`bld-kernel` crate, whose determinism guarantees are typed.

## Status

**The tool is functionally complete:** the YAML spec format, `bld topology
validate`/`render`/`verify`, `bld scaffold domain`/`adversarial`/`probe`, and a
**`bld-mcp` server** that exposes all of it to an agent over MCP. Work is staged
(see [`docs/design.md`](docs/design.md)):

1. **Stage 1** — the YAML spec format + `bld topology validate` / `render` — ✅ done
2. **Stage 2** — `bld scaffold domain` / `scaffold adversarial` — ✅ done
3. **Stage 3** — the MCP server + a "map your domain" prompt — ✅ done
4. **Stage 4** — `bld topology verify` + `scaffold probe` (prove the shipped Rust
   still matches the spec) — ✅ done · **publishing `bld-kernel` to crates.io is
   deferred** and needs the owner's explicit sign-off (it is irreversible)

## Quick start

```bash
# Validate a domain spec — totality + illegal-edge soundness (exit 1 on errors):
cargo run -p bld -- topology validate examples/domain.example.yaml

# Render topology.json + a Mermaid state diagram (topology.mmd):
cargo run -p bld -- topology render examples/domain.example.yaml

# Scaffold a Rust impl BoundaryDomain skeleton + its adversarial harness + a probe:
cargo run -p bld -- scaffold domain      examples/domain.example.yaml
cargo run -p bld -- scaffold adversarial examples/domain.example.yaml
cargo run -p bld -- scaffold probe       examples/domain.example.yaml

# Verify a domain's exported topology.json still matches the spec (catch drift):
cargo run -p bld -- topology verify examples/domain.example.yaml topology.json

cargo run -p bld -- help
```

The scaffold writes every state, input, effect and legal transition, leaves the
guards and per-state data as `TODO`, and makes every illegal transition *absent*
(`Undefined`). Full compilation against `bld-kernel` is verified at Stage 4.

## Use it as an MCP server

`bld-mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server
(stdio, JSON-RPC 2.0) that exposes the whole flow to an agent, so another team can
map a domain and replicate the boundary from inside their coding assistant.

### Setup

Build the binary, then register it with your MCP client:

```bash
cargo build --release -p bld-mcp        # → target/release/bld-mcp
```

**Claude Code:**

```bash
claude mcp add bld -- /absolute/path/to/target/release/bld-mcp
```

**Any client using an `mcpServers` config** (Claude Desktop, Cursor, …):

```json
{
  "mcpServers": {
    "bld": { "command": "/absolute/path/to/target/release/bld-mcp" }
  }
}
```

It speaks stdio JSON-RPC and needs no network, no config, and no environment.

### What it exposes

**Tools** — each takes one argument, `spec` (the domain YAML), and returns text:

| Tool | Returns |
|---|---|
| `topology_validate` | a per-door report + a `VALID` / `INVALID` verdict |
| `topology_render` | `topology.json` (the published graph) + a Mermaid state diagram |
| `scaffold_domain` | a Rust `impl BoundaryDomain` skeleton |
| `scaffold_adversarial` | a Rust test asserting every illegal transition is absent |

A spec that does not parse is the only tool *error*; a spec that parses but is
unsound is a normal result whose text explains what is wrong.

**Resources** — `bld://contract` (the BLD model and how to shape a spec — an agent
should read this first) and `bld://example` (the commented spec template).

**Prompt** — `map_domain` (optional `description` argument): interviews the user and
produces a validated spec.

### A worked flow

An agent modelling, say, a loan-approval domain typically:

1. reads `bld://contract` and `bld://example` to learn the model and the spec shape;
2. optionally runs the `map_domain` prompt to interview the user;
3. drafts a `spec` and calls `topology_validate`, fixing what it reports until `VALID`;
4. calls `scaffold_domain` and `scaffold_adversarial` for the `BoundaryDomain`
   skeleton and its adversarial test;
5. calls `topology_render` to publish the graph for human review.

The agent never touches the filesystem — every tool takes the spec as text and
returns text.

Any state × input pair the spec does not name is a `no_edge` — a transition that
**does not exist**, not one refused at runtime. `validate` proves that grid is
total and every authored edge is sound; `render` publishes it so an adversary (and
your reviewer) has the map.

- The design note: [`docs/design.md`](docs/design.md)
- A commented, domain-neutral spec template: [`examples/domain.example.yaml`](examples/domain.example.yaml)

## License

Dual-licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at
your option.
