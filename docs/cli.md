# CLI contract

## Reference CLI

```bash
python3 reference/proofrun_ref.py doctor
python3 reference/proofrun_ref.py plan --base <rev> --head <rev> --profile ci
python3 reference/proofrun_ref.py explain --plan .proofrun/plan.json
python3 reference/proofrun_ref.py emit shell --plan .proofrun/plan.json
python3 reference/proofrun_ref.py emit github-actions --plan .proofrun/plan.json
python3 reference/proofrun_ref.py run --plan .proofrun/plan.json --dry-run
```

## Target Cargo subcommand

```bash
cargo proofrun plan --base <rev> --head <rev> --profile ci
cargo proofrun explain --plan .proofrun/plan.json
cargo proofrun explain --plan .proofrun/plan.json --solver
cargo proofrun explain --plan .proofrun/plan.json --solver --surface workspace.all-tests
cargo proofrun run --plan .proofrun/plan.json
cargo proofrun emit shell --plan .proofrun/plan.json
cargo proofrun emit github-actions --plan .proofrun/plan.json
cargo proofrun doctor
```

`cargo proofrun explain --solver` reconstructs the candidate surface set from the current checked-in config and the saved plan. It is intended to answer why a surface was selected or omitted, with cost and coverage context. If the current repo policy no longer matches the plan's `config_digest`, the command fails instead of inventing an explanation from drifted config.

## Outputs

`plan` writes:

- `.proofrun/diff.patch`
- `.proofrun/plan.json`
- `.proofrun/plan.md`
- `.proofrun/commands.sh`
- `.proofrun/github-actions.yml`

`run` writes:

- `.proofrun/receipt.json`
- `.proofrun/logs/*`
