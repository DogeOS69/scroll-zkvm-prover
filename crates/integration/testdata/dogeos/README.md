# DogeOS Integration Testdata

Store DogeOS-only integration fixtures here so they are clearly separated from
Scroll's upstream fixture sets.

Suggested layout:

- `crates/integration/testdata/dogeos/<feature>/witnesses/...`
- `crates/integration/testdata/dogeos/<feature>/tasks/...`
- `crates/integration/testdata/dogeos/<feature>/proofs/...`

Each feature directory should include a short note describing provenance and
regeneration steps.
