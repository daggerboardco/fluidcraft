# Fluidcraft

A CFD (Computational Fluid Dynamics) application written in **Rust**, using
**OpenFOAM** as the compute backend.

Fluidcraft orchestrates OpenFOAM's meshing and solving tools as subprocesses —
no FFI. It targets a Linux-native, parallel, and GPU-capable workflow, with
mesh creation as the core focus.

> **Inspiration vs. IP.** The meshing workflow is inspired by general techniques
> used by mainstream commercial CFD applications (octree/delaunay tessellation,
> polyhedral conversion, prism inflation layers, y+ control). These are standard,
> non-proprietary engineering concepts. Fluidcraft does **not** copy, port, or
> reverse-engineer any proprietary code, binaries, or assets. All utilized tools
> are open source, public domain, or used under fair use. See
> [`roadmap.md#7-licensing--copyright-compliance`](roadmap.md#7-licensing--copyright-compliance)
> for the binding rules.

---

## Features

- **OpenFOAM backend** — `blockMesh`, `snappyHexMesh`, solvers, and
  `decomposePar` invoked as subprocesses.
- **Case scaffolding** — generate a valid OpenFOAM case skeleton with
  `fluidcraft new`.
- **CLI-driven** — `check`, `new`, `mesh`, `solve` subcommands.
- **Rust throughout** — shell, geometry, orchestration, and (future) GPU backend.
- **Parallel-ready** — MPI domain decomposition via `decomposePar` (metis).
- **GPU roadmap** — CPU baseline first, GPU acceleration later.

## Architecture

```
┌─────────────────────────────────────────────────┐
│                 Fluidcraft (Rust)                │
│                                                  │
│  CLI (clap) → Orchestration (tokio) → OpenFOAM   │
│        │                    │                    │
│        │                    ▼                    │
│        │          subprocess (no FFI)            │
│        │                    │                    │
│        ▼                    ▼                    │
│   geometry import    mesh + solve (blockMesh,   │
│   (OCC / cadquery)    snappyHexMesh, solvers)   │
└─────────────────────────────────────────────────┘
```

OpenFOAM is treated as a CLI library: Fluidcraft generates case files and runs
the OpenFOAM tools as subprocesses. This keeps Fluidcraft FFI-free and makes the
GPU backend a drop-in swap for the compute step.

## Installation

### Prerequisites

- Rust toolchain (edition 2021+, tested on 1.98).
- OpenFOAM — install a distro build (`openfoam` / `distro-clean`) and **source
  its `bashrc`** so `blockMesh`, solvers, etc. are on `PATH`.

### Build

```bash
cargo build --release
```

The binary is produced at `target/debug/fluidcraft` (debug) or
`target/release/fluidcraft` (release).

## Usage

```bash
# Verify OpenFOAM is available and report the version
fluidcraft check

# Generate an OpenFOAM case directory
fluidcraft new /path/to/case

# Run the meshing pipeline (blockMesh + snappyHexMesh)
fluidcraft mesh /path/to/case

# Run a solver over a case directory
fluidcraft solve /path/to/case --solver simpleFoam
```

### Example end-to-end

```bash
fluidcraft new ./my-case
# ... populate geometry, boundary conditions, materials ...
fluidcraft mesh ./my-case
fluidcraft solve ./my-case --solver simpleFoam
```

## Project layout

```
Fluidcraft/
├── Cargo.toml        # Phase 0 crate stack
├── roadmap.md        # Full project plan (phases, risks, licensing)
├── LICENSE           # MIT (Fluidcraft's own code)
├── src/
│   ├── main.rs       # Entry point + tokio runtime
│   ├── cli.rs        # clap CLI: check / new / mesh / solve
│   ├── error.rs      # Typed error enum
│   └── of.rs         # OpenFOAM subprocess orchestration
└── .github/          # CI + issue/PR templates (GitHub)
```

## Roadmap

The full plan lives in [`roadmap.md`](roadmap.md). Summary of phases:

| Phase | Title | Focus |
|---|---|---|
| 0 | Toolchain & Spike | Build baseline; `blockMesh` → solve → results |
| 1 | Geometry Import | CAD/STEP import (OCC or cadquery) |
| 2 | Meshing | `blockMesh` + `snappyHexMesh` orchestration |
| 3 | Solver Orchestration | Parallel OpenFOAM solves |
| 4 | GPU Backend | GPU-accelerated compute |
| 5 | UI / 3D Rendering | `egui`/`iced` + `wgpu`/`bevy` |
| 6 | Polish | Pre/post-processing, presets, docs |

## Contributing

Contributions are welcome! See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

Fluidcraft's own code is licensed under the [MIT License](LICENSE). OpenFOAM is
distributed under **GPL-v3** and is used here as an external subprocess — see
[`roadmap.md#7`](roadmap.md#7-licensing--copyright-compliance) for details.
