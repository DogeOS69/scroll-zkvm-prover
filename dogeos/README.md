# DogeOS Fork Workflow

This fork stays maintainable by treating Scroll's `master` as the source of
truth and keeping DogeOS work as a thin overlay.

## Branch model

- `master`: exact mirror of `upstream/master`. Do not add DogeOS commits here.
- `dogeos/main`: canonical DogeOS base branch. Keep it small, linear, and easy
  to rebase.
- `dogeos/<feature>`: feature overlays opened as PRs into `dogeos/main`.

## Update workflow

1. `git fetch upstream origin --prune`
2. `git checkout master`
3. `git merge --ff-only upstream/master`
4. `git push origin master`
5. `git checkout dogeos/main`
6. `git rebase master`
7. `git push --force-with-lease origin dogeos/main`
8. `git checkout dogeos/<feature>`
9. `git rebase dogeos/main`
10. `git push --force-with-lease origin dogeos/<feature>`

## Dependency policy

- When a DogeOS feature depends on `stateless-block-verifier`, patch all
  required `sbv-*` crates together instead of mixing forked and upstream git
  sources.
- Pin the DogeOS fork by immutable `rev` or tag in `Cargo.toml`.
- When the SBV commit changes, update both the `rev` and the corresponding
  `Cargo.lock` source entries together.
- Before the SBV commit is pushed, use temporary CLI `cargo --config` path
  overrides for local verification instead of checking in `.cargo/config.toml`.

## Testdata policy

- Put DogeOS-only integration assets under
  `crates/integration/testdata/dogeos/<feature>/`.
- If a feature needs extra prover or verifier assets, add sibling `dogeos/`
  subdirectories under the existing crate-local `testdata/` trees.
- Keep feature-specific notes under `dogeos/`.
