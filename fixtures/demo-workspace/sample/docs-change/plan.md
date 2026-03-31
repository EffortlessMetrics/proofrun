# proofrun plan

- range: `ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb..1dcb4c3fd4154e4d7cd12bcff0101679f84fc1dd`
- merge base: `ae21fea001cb9fc82101f7368ec5cfb4a6fa46eb`
- profile: `ci`
- plan digest: `76bbed98f4c351b58080b822b279043a1338c1170bc1060d68fa0c36a2c23e7f`

## Changed paths

- `M` `docs/guide.md` → `unowned`

## Obligations

- `workspace:docs`
  - source=rule, path=docs/guide.md, rule=rule:3, pattern=docs/**
- `workspace:smoke`
  - source=profile, path=None, rule=ci, pattern=None

## Selected surfaces

- `workspace.docs` — cost `4.0`
  - covers: workspace:docs
  - run: `cargo doc --workspace --no-deps`
- `workspace.smoke` — cost `2.0`
  - covers: workspace:smoke
  - run: `cargo test --workspace --quiet`
