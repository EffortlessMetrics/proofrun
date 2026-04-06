# RELEASE

This file tracks the publication path for the Rust CLI.

## Publication order

1. Publish the core library crate used by the CLI:
   - `crates/proofrun` (`proofrun`)
2. Publish the public CLI crate:
   - `crates/cargo-proofrun` (`cargo-proofrun`)
3. Keep remaining internal crates non-publishable:
   - `xtask`

## Release preflight (run locally)

1. Update versioning and changelog.
2. Regenerate fixtures if behavior has changed.
3. Validate release packaging:

```bash
cargo publish --dry-run -p proofrun
cargo package --list -p proofrun
cargo publish --dry-run -p cargo-proofrun
cargo package --list -p cargo-proofrun
```

4. Verify golden/reference behavior for the release slice.
5. Tag a signed alpha release:

```bash
git tag -a v0.1.0-alpha.1 -m "release: cargo-proofrun 0.1.0-alpha.1"
git push origin v0.1.0-alpha.1
```

6. Trigger GitHub `release` workflow (or push the tag to publish release binaries).

## Post-release

1. Publish crates to crates.io (`proofrun` then `cargo-proofrun`).
2. Update `README` for the new release boundary.
3. Cut the next profile/alpha plan.
