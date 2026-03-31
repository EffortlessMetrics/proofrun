# AGENTS

This repo is structured so an agent can make bounded changes without inventing its own rules.

## What to read first

1. `README.md`
2. `docs/architecture.md`
3. `docs/policy.md`
4. `docs/scenario-atlas.md`
5. `TESTING.md`

## Crate and module map

- `reference/proofrun_ref.py`
  - runnable reference implementation
  - authoritative for current behavior in this bundle

- `crates/proofrun`
  - target Rust library architecture
  - planning model, config, git adapter, workspace adapter, solver, emitters

- `crates/cargo-proofrun`
  - target Cargo subcommand surface

- `crates/xtask`
  - target repo ritual layer

## Commands

```bash
python3 reference/proofrun_ref.py doctor
python3 reference/proofrun_ref.py plan --base <rev> --head <rev> --profile ci
python3 reference/proofrun_ref.py explain --plan .proofrun/plan.json
python3 reference/proofrun_ref.py emit shell --plan .proofrun/plan.json
python3 reference/proofrun_ref.py run --plan .proofrun/plan.json --dry-run
python3 -m unittest discover -s tests -p 'test_*.py'
```

## Stop and escalate when

- a change would require new config schema semantics
- a change would alter the plan or receipt artifact shape
- a change would weaken fail-closed behavior
- a change would introduce network or service requirements
- a change would widen scope beyond Cargo workspaces without an explicit design update

## Proof expectations

Any change that affects planning behavior should update:

- at least one scenario or fixture
- the scenario atlas if behavior surface changes
- sample artifacts if artifact shape changes
- JSON schemas if plan or receipt contracts change
