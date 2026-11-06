# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Project scaffolding (Phase 0).
- CLI subcommands: `check`, `new`, `mesh`, `solve`.
- `geometry convert` subcommand: import a STEP model into a normalized ASCII
  STL via the solidcraft CLI (the solidcore / SMLib Rust-port kernel). Handles
  binary + ASCII STL, solidcraft's 80-byte-header hybrid format, and unit
  scaling (`--scale`, e.g. 0.001 for mm -> m).
- Geometry-import module (`src/geometry.rs`) with STL read/write, bounding-box
  helpers, and solidcraft binary auto-discovery (`--solidcraft` / `$FLUIDCRAFT_SOLIDCRAFT` / PATH).
- OpenFOAM case scaffolding via `fluidcraft new`.
- MIT `LICENSE` and dependency-license manifest (`LICENSES/DEPENDENCIES.md`).
- CI workflow (build + clippy on Linux).
- Overwrite guard for `fluidcraft new` (`--overwrite` flag; refuses to clobber
  an existing case).
- Absolute path resolution for OpenFOAM case directories.
- `snappyHexMesh` added to the `fluidcraft mesh` pipeline
  (`blockMesh` → `snappyHexMesh`).
- Admin-bypass merge policy documented in CONTRIBUTING.md.
- Dependency & licensing documentation for the geometry kernel: solidcore
  (Apache-2.0, NVIDIA; MIT components) and solidcraft (Apache-2.0) documented in
  `LICENSES/DEPENDENCIES.md` and `licensing.md`, including tangency-labs' explicit
  authorization and the obligation to keep the local forks linked to the
  `tangency-labs` upstream.
- Phase 3 (solver orchestration) completed: transient PIMPLE solver integrated
  with a passing acceptance gate (net mass imbalance ~-1.6e-11 m³/s, max
  Courant 0.0316, 0 NaN, single connected mesh region).
- CUDA toolkit 13.4.3 installed (`/opt/cuda`, `nvcc` on PATH) and verified on the
  NVIDIA RTX PRO 5000 72GB (Blackwell, sm_120) — Phase 4 (GPU backend) toolchain
  ready. OpenFOAM-14 itself has no GPU backend, so GPU work is a parallel
  accelerator path with the CPU solve as baseline.
- Roadmap updated: Phases 1 & 3 marked done, Phase 4 marked in progress with the
  CUDA install and the no-GPU-in-OpenFOAM constraint, and an upstream-drift risk
  added for solidcore/solidcraft.
