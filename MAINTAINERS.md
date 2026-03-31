# MAINTAINERS

## Maintainer posture

`proofrun` is a trust surface, not just a utility. Review artifacts and reasoning, not only code shape.

## Merge criteria

A planning change is ready when:

- the reference implementation still passes its test suite
- affected fixture outputs are updated intentionally
- plan determinism is preserved
- schema changes are versioned and documented
- fail-closed behavior remains explicit

## Non-goals for v0.1

- predictive test selection
- cloud-only services
- workflow dashboards
- generalized build-system replacement

## Release checks

Before cutting a release:

1. run unit and end-to-end tests
2. regenerate sample outputs
3. diff `schema/*.json`
4. verify docs and scenario atlas still match behavior
5. verify `reference/proofrun_ref.py --help` output
