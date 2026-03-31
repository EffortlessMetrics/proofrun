# Project Structure

```
proofrun/
├── reference/
│   └── proofrun_ref.py          # Authoritative reference implementation (stdlib-only Python)
│
├── crates/
│   ├── proofrun/                # Target Rust planning library
│   │   └── src/
│   │       ├── lib.rs           # Library root
│   │       ├── model.rs         # Domain types (signals, obligations, surfaces)
│   │       ├── config.rs        # proofrun.toml parsing
│   │       ├── planner.rs       # Set-cover solver
│   │       ├── obligations.rs   # Obligation derivation from signals
│   │       ├── git.rs           # Git adapter (diff, changed paths)
│   │       ├── cargo_workspace.rs # Cargo workspace discovery
│   │       ├── emit.rs          # Artifact emitters (plan.json, commands.sh, etc.)
│   │       ├── explain.rs       # Human-readable plan explanation
│   │       ├── run.rs           # Plan execution and receipt generation
│   │       └── doctor.rs        # Environment health checks
│   │
│   ├── cargo-proofrun/          # Cargo subcommand binary (`cargo proofrun`)
│   │   └── src/main.rs
│   │
│   └── xtask/                   # Repo ritual automation
│       └── src/main.rs
│
├── schema/                      # Versioned JSON Schemas (config, plan, receipt)
├── docs/                        # Architecture, policy, CLI, scenarios, roadmap
├── examples/                    # Example proofrun.toml
├── fixtures/demo-workspace/     # Sample Cargo workspace with git history
│   ├── repo/                    # The fixture workspace itself
│   └── sample/                  # Pre-generated plan/receipt artifacts per scenario
├── tests/                       # Python unit tests for reference implementation
├── proofrun.toml                # This repo's own proof policy
├── justfile                     # Task runner recipes
├── Cargo.toml                   # Workspace manifest
└── rust-toolchain.toml          # Pinned stable toolchain + clippy/rustfmt
```

## Key conventions

- `reference/proofrun_ref.py` is the source of truth for current behavior. The Rust crates mirror its architecture but are the target production path.
- Adapters (git, cargo workspace, process execution) live at the edges. The planning core (`planner.rs`, `obligations.rs`, `model.rs`) must stay pure and deterministic.
- Fixtures in `fixtures/demo-workspace/sample/` contain golden artifacts. Changes to planning logic must regenerate and review these.
- JSON schemas in `schema/` are the contract for plan and receipt artifacts. Any shape change requires a schema update.
- `docs/scenario-atlas.md` documents expected planning behavior per scenario. Update it when behavior surface changes.
