# Reference implementation

This is the runnable implementation in this bundle.

## Why it exists

The target product is a Cargo subcommand implemented in Rust. This bundle was prepared in an environment without a Rust toolchain, so the Python reference implementation gives you a working artifact immediately while preserving the intended production layout in `crates/`.

## Properties

- Python stdlib only
- reads `proofrun.toml`
- scans a Cargo workspace by manifest files
- shells out to Git for range and patch data
- solves an exact weighted cover over candidate surfaces
- writes plan and receipt artifacts

## Example

```bash
python3 reference/proofrun_ref.py plan --base <rev> --head <rev> --profile ci
python3 reference/proofrun_ref.py explain --plan .proofrun/plan.json
python3 reference/proofrun_ref.py run --plan .proofrun/plan.json --dry-run
```
