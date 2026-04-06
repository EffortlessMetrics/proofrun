# proofrun plan

- range: `8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717..8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717`
- merge base: `8a98e3f3f44b8fcb4ae8f853911b4c5bb1d26717`
- profile: `ci`
- plan digest: `871bbf5dc88f5afe6166185ef8ae198b431caa08da598dde70be926719b778d6`

## Changed paths


## Obligations

- `workspace:smoke`
  - source=profile, path=None, rule=ci, pattern=None

## Selected surfaces

- `workspace.smoke` — cost `2.0`
  - covers: workspace:smoke
  - run: `cargo test --workspace --quiet`
