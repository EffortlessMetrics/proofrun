# Tech Stack

## Languages

- Rust (stable toolchain) — target production implementation
- Python 3 (stdlib only) — reference implementation, no external dependencies

## Rust workspace

- Edition: 2021
- Resolver: 2
- Toolchain components: clippy, rustfmt (via `rust-toolchain.toml`)
- License: Apache-2.0 OR MIT

## Key Rust dependencies

- `anyhow` — error handling
- `camino` — UTF-8 paths (with serde support)
- `clap` (derive) — CLI argument parsing
- `serde` / `serde_json` / `toml` — serialization
- `sha2` — hashing
- `thiserror` — typed errors
- `time` — timestamps

## Build & task tools

- `cargo` — Rust build system
- `just` — task runner (justfile at repo root)
- `cargo-nextest` — test runner (config in `.config/nextest.toml`)
- `cargo-mutants` — mutation testing (config in `.cargo/mutants.toml`)

## Schemas

JSON Schema files in `schema/` define contracts for:
- `config.schema.json` — proofrun.toml config
- `plan.schema.json` — plan artifact
- `receipt.schema.json` — receipt artifact

## Common commands

```bash
# Reference implementation
python3 reference/proofrun_ref.py doctor
python3 reference/proofrun_ref.py plan --base <rev> --head <rev> --profile ci
python3 reference/proofrun_ref.py explain --plan .proofrun/plan.json
python3 reference/proofrun_ref.py emit shell --plan .proofrun/plan.json
python3 reference/proofrun_ref.py run --plan .proofrun/plan.json --dry-run

# Tests (reference implementation)
python3 -m unittest discover -s tests -p 'test_*.py'

# Rust build
cargo build --workspace
cargo clippy --workspace
cargo fmt --check

# Just shortcuts
just test
just doctor
just plan <base> <head>
just explain
just dry-run
```

## Escalation triggers

Stop and escalate (do not proceed) when a change would:
- Require new config schema semantics
- Alter plan or receipt artifact shape
- Weaken fail-closed behavior
- Introduce network or service requirements
- Widen scope beyond Cargo workspaces without an explicit design update
