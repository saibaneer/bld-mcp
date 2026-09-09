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

**Stages 1–2 are done:** the YAML spec format, `bld topology validate`/`render`,
and `bld scaffold domain`/`adversarial` (a faithful `impl BoundaryDomain` skeleton
+ an adversarial harness, generated from the same validated topology). Work is
staged (see [`docs/design.md`](docs/design.md)):

1. **Stage 1** — the YAML spec format + `bld topology validate` / `render` — ✅ done
2. **Stage 2** — `bld scaffold domain` / `scaffold adversarial` — ✅ done
3. **Stage 3** — the MCP server wrapping the CLI + a "map your domain" prompt — ← next
4. **Stage 4** — verify the shipped Rust still matches the spec (and publish `bld-kernel`)

## Quick start

```bash
# Validate a domain spec — totality + illegal-edge soundness (exit 1 on errors):
cargo run -p bld -- topology validate examples/domain.example.yaml

# Render topology.json + a Mermaid state diagram (topology.mmd):
cargo run -p bld -- topology render examples/domain.example.yaml

# Scaffold a Rust impl BoundaryDomain skeleton + its adversarial harness:
cargo run -p bld -- scaffold domain      examples/domain.example.yaml
cargo run -p bld -- scaffold adversarial examples/domain.example.yaml

cargo run -p bld -- help
```

The scaffold writes every state, input, effect and legal transition, leaves the
guards and per-state data as `TODO`, and makes every illegal transition *absent*
(`Undefined`). Full compilation against `bld-kernel` is verified at Stage 4.

Any state × input pair the spec does not name is a `no_edge` — a transition that
**does not exist**, not one refused at runtime. `validate` proves that grid is
total and every authored edge is sound; `render` publishes it so an adversary (and
your reviewer) has the map.

- The design note: [`docs/design.md`](docs/design.md)
- A commented, domain-neutral spec template: [`examples/domain.example.yaml`](examples/domain.example.yaml)

## License

Dual-licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at
your option.
