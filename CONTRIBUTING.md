# Contributing to Fluidcraft

Thanks for your interest in contributing to Fluidcraft! This document covers the
workflow, conventions, and expectations for contributions.

## Getting started

1. Fork and clone the repository.
2. Install the Rust toolchain (edition 2021+).
3. Install OpenFOAM and source its `bashrc` so the tools are on `PATH`.
4. Create a feature branch: `git checkout -b feat/my-feature`.

## Development workflow

- Keep changes focused; one logical change per commit.
- Follow the phase plan in [`roadmap.md`](roadmap.md). Don't build beyond the
  current phase without discussion.
- Verify locally before opening a PR (see below).

## Verifying locally

```bash
cargo build                 # compiles
cargo test                  # runs tests (as they are added)
cargo clippy -- -D warnings # lint clean
cargo fmt                   # formatting
```

For end-to-end validation, run a minimal OpenFOAM case:

```bash
cargo run -- new ./scratch-case
cargo run -- mesh ./scratch-case
cargo run -- solve ./scratch-case --solver simpleFoam
```

## Conventions

- **Language:** Rust throughout (shell, geometry, orchestration, GPU backend).
- **OpenFOAM as CLI:** orchestrate OpenFOAM tools as subprocesses; do not add
  FFI bindings unless explicitly required.
- **Don't reimplement the mesher.** Orchestrate OpenFOAM's meshers.
- **Naming:** snake_case for Rust code; match OpenFOAM's own casing for dict
  keys and field names.
- **Licensing:** every dependency must be open source, public domain, or fair
  use. Update [`LICENSES/DEPENDENCIES.md`](LICENSES/DEPENDENCIES.md) when adding
  a crate. See [licensing.md](licensing.md) for the binding rules.

## Opening a pull request

1. Describe the change and its motivation in the PR body.
2. Reference the phase/issue it addresses.
3. Confirm the local verification steps above pass.
4. Keep the PR scoped; split larger efforts into focused PRs.

## Code of conduct

By participating, you are expected to uphold our [Code of Conduct](CODE_OF_CONDUCT.md).
