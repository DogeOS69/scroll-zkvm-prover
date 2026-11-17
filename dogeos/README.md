# DogeOS Fork Workflow

This fork stays maintainable by treating Scroll's `master` as the source of
truth and keeping DogeOS work as a thin overlay.

## Branch model

- `master`: exact mirror of `upstream/master`. Do not add DogeOS commits here.
- `dogeos/main`: canonical DogeOS base branch. Keep it small, linear, and easy
  to rebase.
- `dogeos/<feature>`: feature overlays opened as PRs into `dogeos/main`.

## Update workflow

1. `git fetch --all --prune`
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

## Tsuki v0.14 dependency pin

All five SBV crates use immutable commit
`1f9a00eda1716259f367d55c6ab33a6182c94639`. It selects dogeos-reth tag `tsuki-v0.14`
(`4788802d646dd177577f95d276122e0029ae5d23`, merged PR #59) and Reth tag
`tsuki-v0.14` (`005acc083ccad7c6d76b28e2da8cbe7593cc3a14`). DogeOS REVM
remains on `tsuki-v0.13`; OpenVM remains on `v1.7.0`.

Run `make test-chunk-witness-checks` before rebuilding guest assets. This
repin retains main's codec V12 semantics and adds no checkpoint configuration.
Guest executable commitments require canonical `make build-guest` verification
before deployment. For CI artifacts, verify `guest-build-source.txt` against
the tested source commit and `guest-build-inputs.sha256` against the checked-out
inputs before applying `guest-build.diff`. Native replay and guest VM execution
do not establish proof generation or verification.

## Testdata policy

- Put DogeOS-only integration assets under
  `crates/integration/testdata/dogeos/<feature>/`.
- If a feature needs extra prover or verifier assets, add sibling `dogeos/`
  subdirectories under the existing crate-local `testdata/` trees.
- Keep feature-specific notes under `dogeos/`.
