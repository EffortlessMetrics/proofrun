# Policy model

## Design rule

Rules emit **obligations**, not commands.

Surfaces cover obligations. The solver selects surfaces.

## Example

```toml
[[rule]]
when.paths = ["crates/*/src/**/*.rs"]
emit = ["pkg:{owner}:tests", "pkg:{owner}:mutation-diff"]

[[surface]]
id = "tests.pkg"
covers = ["pkg:{pkg}:tests"]
cost = 3
run = ["cargo", "nextest", "run", "--profile", "{profile}", "-E", "package({pkg})"]
```

## Why this matters

This keeps planning composable.

A path change does not directly say "run this command." It says "this change owes these proofs." Multiple surfaces may discharge the same obligation, and the planner can choose the cheapest valid set.

## Unknown ownership

The default posture is fail-closed.

If a rule emits an obligation that requires an owning package but the changed path cannot be owned, `proofrun` should fall back to explicit safety-net obligations and emit a clear diagnostic.
