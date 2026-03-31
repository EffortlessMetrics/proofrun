# TESTING

## Test layers

### Reference implementation tests

Run:

```bash
python3 -m unittest discover -s tests -p 'test_*.py'
```

These cover:

- set-cover planner behavior
- config loading
- fixture repo end-to-end planning
- artifact emission

### Fixture scenarios

The canonical fixture repo is:

- `fixtures/demo-workspace/repo`

Recorded scenarios:

- `initial -> core_change`
- `core_change -> docs_change`

Artifacts live in:

- `fixtures/demo-workspace/sample/core-change/`
- `fixtures/demo-workspace/sample/docs-change/`

### Manual smoke flow

```bash
cd fixtures/demo-workspace/repo
python3 ../../../reference/proofrun_ref.py plan --base <rev> --head <rev> --profile ci
python3 ../../../reference/proofrun_ref.py explain --plan .proofrun/plan.json
python3 ../../../reference/proofrun_ref.py emit shell --plan .proofrun/plan.json
python3 ../../../reference/proofrun_ref.py run --plan .proofrun/plan.json --dry-run
```

## Updating samples

When planning logic changes:

1. regenerate sample artifacts from the fixture repo
2. review `plan.json` and `plan.md`
3. update `docs/scenario-atlas.md` if the semantic surface changed
4. update schemas if artifact shape changed
