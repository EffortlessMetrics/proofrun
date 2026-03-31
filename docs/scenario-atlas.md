# Scenario atlas

This is the initial semantic index for the reference implementation.

| Scenario | Problem | Expected obligations | Primary artifacts |
|---|---|---|---|
| docs-only change | do not spend code-test compute on prose-only edits | `workspace:docs`, `workspace:smoke` | `plan.json`, `plan.md` |
| leaf crate code change | local code change needs package proof plus mutation slice | `pkg:<owner>:tests`, `pkg:<owner>:mutation-diff`, `workspace:smoke` | `plan.json`, `commands.sh` |
| manifest change | dependency or feature shifts can widen blast radius | `pkg:<owner>:tests`, `pkg:<owner>:rdeps`, `workspace:smoke` | `plan.json`, `plan.md` |
| unknown owner | ambiguous ownership must fail closed | fallback obligations from `[unknown]` | `plan.json`, diagnostics |
| many changed packages | broader workspace surface may be cheaper than many narrow surfaces | exact weighted cover chooses `workspace.all-tests` if cheaper | `plan.json` |
| dry-run execution | handoff without local toolchain execution | `receipt.json` with `dry-run` status | `receipt.json` |

## Fixture scenarios

Fixture history lives in `fixtures/demo-workspace/repo`.

| Range | Scenario |
|---|---|
| `initial -> core_change` | leaf crate code change |
| `core_change -> docs_change` | docs-only change |
