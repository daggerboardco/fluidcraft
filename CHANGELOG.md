# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Project scaffolding (Phase 0).
- CLI subcommands: `check`, `new`, `mesh`, `solve`.
- OpenFOAM case scaffolding via `fluidcraft new`.
- MIT `LICENSE` and dependency-license manifest (`LICENSES/DEPENDENCIES.md`).
- CI workflow (build + clippy on Linux).
- Overwrite guard for `fluidcraft new` (`--overwrite` flag; refuses to clobber
  an existing case).
- Absolute path resolution for OpenFOAM case directories.
- `snappyHexMesh` added to the `fluidcraft mesh` pipeline
  (`blockMesh` → `snappyHexMesh`).
- Admin-bypass merge policy documented in CONTRIBUTING.md.
