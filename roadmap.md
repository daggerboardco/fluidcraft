# Fluidcraft — Project Roadmap

> A CFD application that uses **OpenFOAM** as the compute backend — Linux-native, parallel, and GPU-capable. Mesh creation is the core focus.

**Language:** Rust (throughout — shell, geometry, orchestration, GPU backend).
**Solver / meshing engine:** OpenFOAM (invoked as a subprocess library).
**Goal:** Build a Linux-native CFD workflow with an OpenFOAM backend and GPU acceleration.

> **Inspiration vs. IP.** The meshing workflow is inspired by general techniques used by commercial CFD applications (octree/delaunay tessellation, polyhedral conversion, prism inflation layers, y+ control). These are standard, non-proprietary engineering concepts. See Section 7 (Licensing and Copyright) for the binding rules.

---

## 0. Background — The Reference Meshing Workflow

General findings about the meshing approach used by mainstream commercial CFD
applications (observed as inspiration only — no proprietary code or binaries are
used, copied, or ported):

- **Architecture:** commercial CFD apps typically pair a native C++ solver/mesher
  with a managed UI layer. This project instead pairs a Rust shell with the
  open-source OpenFOAM engine.
- **Mesher is native C++** in those products, with proprietary algorithms —
  reimplementing them from decompiled output would risk IP infringement. **Do
  not reimplement or port a proprietary mesher.**
- **Geometry import formats (open, standards-based):** STEP, IGES, Parasolid/SAT,
  NX/UG, CATIA, ProE/CREO, SolidWorks, plus STL, OBJ, DXF, JT. We support these
  via open standards and open-source converters, not proprietary readers.
- **Meshing recipe (standard engineering techniques):**
  - Algorithm: **octree-dominant** + **delaunay** + **advancing-front**, with
    **polyhedral conversion** and **non-conformal interface matching**.
  - Elements: **tetrahedral core**; **wedge/prism/pyramid** for near-wall; shells.
  - Boundary layers: **inflation layers** with **y+ targeting**, **wall functions**,
    **k-ω SST** turbulence.
  - Sizing: global/local **sizing**, **growth rate**, **transition**, **AMR**
    (adaptive refinement), quality/skewness control.
  - Parallel: **MPI** + **metis domain decomposition** + partitioning.
- **Key insight:** OpenFOAM already implements these standard techniques.
  `snappyHexMesh` = polyhedral core + prism inflation layers + y+;
  `blockMesh` = hex core; `decomposePar` (metis) = MPI decomposition;
  `refineMesh` = AMR. **Our IP is the app shell + geometry + GPU backend, not the
  meshing algorithm.**

---

## 1. Guiding Principles

1. **Don't reimplement or port a proprietary mesher.** Orchestrate OpenFOAM's
   meshers, which already implement the standard meshing recipe.
2. **Rust owns the shell.** Geometry, meshing orchestration, solver calls, GPU
   backend, and UI all in Rust.
3. **OpenFOAM is a CLI library.** Call `blockMesh`, `snappyHexMesh`, solvers, and
   `decomposePar` as subprocesses — no FFI needed.
4. **De-risk the two hard parts early:** CAD/STEP geometry import (Rust's weakest
   link) and GPU acceleration.
5. **CPU baseline first.** Get a working end-to-end pipeline on CPU before adding
   GPU as an accelerator swap.

---

## 2. Recommended Rust Crate Stack

| Concern | Crates |
|---|---|
| Async runtime | `tokio` |
| CLI / argument parsing | `clap` |
| Config & case serialization | `serde`, `serde_json` |
| Error handling | `anyhow`, `thiserror` |
| Geometry / CAD | `occt` (OCC bindings) or shell out to FreeCAD/cadquery |
| Mesh / geometry model | `trimesh`, `parry3d`, `nalgebra` / `cgmath` |
| Meshing primitives | `octree`, `voro3d` (voro++ FFI) |
| MPI / parallel | `mpi` |
| GPU / CUDA | `cuda-rs`, `rusta_cuda` |
| UI | `egui` (immediate-mode) or `iced` |
| 3D rendering / viz | `wgpu` (low-level) or `bevy` (full 3D engine) |

---

## 3. Phased Roadmap

### Phase 0 — Toolchain & Spike (1 week) ✅ COMPLETED
- Install OpenFOAM (distro build or ESI `openfoam` / `distro-clean`) + ParaView.
- Initialize the Cargo project at `/mnt/970EVO/Projects/Fluidcraft`.
- **Spike:** spawn `blockMesh` → run a solver → parse results end-to-end.
- Establish crate stack: `tokio`, `clap`, `serde`, `anyhow`, `thiserror`.
- **Definition of done:** a Rust binary runs a minimal OpenFOAM case and prints residuals.

### Phase 1 — Geometry Import (2–3 weeks) — *the risk* ✅ COMPLETED
- Rust's CAD ecosystem is immature. Three realistic paths:
  1. **OCC bindings** — `occt` crate (bindgen over OpenCASCADE) for true STEP/IGES/Parasolid in-Rust.
  2. **Shell out** — call FreeCAD's `importPart` or Python `cadquery` as a geometry service (fastest to working).
  3. **STL/OBJ first** — `stl` / `obj-rs` crates as a stopgap, add CAD later.
- **Recommendation:** start with option 2 or 3 to de-risk; graduate to OCC bindings once the pipeline works.
- Normalize imported geometry into a clean B-rep; add surface cleanup (small-face removal, gap/tolerance fixing).
- **Definition of done:** import a STEP file and produce a clean, watertight surface in-Rust.

### Phase 2 — Meshing (3–4 weeks) — *core focus* ✅ COMPLETED
- Orchestrate OpenFOAM CLIs from Rust:
  - `blockMesh` for hex cores / simple domains.
  - `snappyHexMesh` for the standard recipe: **snap → smooth → add prism layers → y+ control**.
- Generate the `snappyHexMeshDict` from Rust (snap/smooth/prism-layer/y+ settings).
- Add local sizing + refinement boxes; run `checkMesh` for quality/skewness/orthogonality.
- Validate mesh output (element mix: polyhedral core + prism walls) against OpenFOAM's own tutorial cases.
- **Definition of done:** produce a polyhedral + prism-layer mesh with y+ control, passing `checkMesh`.

### Phase 3 — Solver Orchestration (2–3 weeks) ✅ COMPLETED
- Wrap OpenFOAM solvers (`simpleFoam`, `kSSTFoam`, etc.) as subprocesses.
- Build case setup: boundary conditions, materials, turbulence models.
- Parallel: `decomposePar` (metis) + `mpirun`/`mpmd` + `recombinePar` — standard MPI partitioning.
  - Use the `mpi` crate for in-process MPI control if desired.
- Job monitoring: parse `log.*`, residuals, y+, forces.
- **Definition of done:** run a parallel OpenFOAM solve and report converged residuals + y+.

### Phase 4 — GPU Backend (3–6 weeks) — *the hard part* ❗IN-PROGRESS
- Evaluate GPU-native solvers that read an OpenFOAM mesh; or OpenFOAM's experimental GPU solvers.
- CUDA via `cuda-rs` / `rusta_cuda`, or wrap an existing GPU solver.
- Keep the CPU path as the baseline; add GPU as an accelerator swap once CPU is solid.
- Benchmark speedup + accuracy vs CPU.
- **Definition of done:** a GPU-accelerated run that matches CPU accuracy with measured speedup.

### Phase 5 — UI / 3D Rendering (2–4 weeks)
- UI framework: **`egui`** (fast, immediate-mode) or **`iced`**.
- 3D visualization: **`wgpu`** (low-level) or **`bevy`** (full 3D engine — ideal for mesh/geometry viewing).
- Integrate ParaView for heavy post-processing.
- **Definition of done:** interactive view of the mesh + live solve monitoring.

### Phase 6 — Polish (ongoing)
- Pre/post-processing UI, mesh quality visualization, case templates.
- Batch / parametric runs, presets, and documentation.

---

## 4. Key Decisions to Make Now

1. **App shell language layer:** Rust is chosen. Confirm Python is only for the
   Phase 1 geometry stopgap (cadquery), not the core.
2. **GPU target:** which solver/library — this drives the backend architecture.
   De-risk in Phase 4 early.
3. **Geometry scope:** full CAD import (OCCT) vs start with STL/already-meshed inputs.

---

## 5. Risk Register

| Risk | Severity | Mitigation |
|---|---|---|
| CAD/STEP import immature in Rust | High | Shell out to cadquery/FreeCAD first; OCC bindings later |
| GPU solver coverage limited | High | CPU baseline first; evaluate GPU-native solvers reading OF meshes |
| snappyHexMesh tuning complexity | Medium | Start from validated OF tutorial cases |
| OpenFOAM version fragmentation | Medium | Pin a specific OpenFOAM build/distro |
| Parallel decomposition quality | Medium | Use metis; validate with `decomposePar` quality metrics |

---

## 6. Success Criteria

- End-to-end pipeline: **STEP → mesh (polyhedral + prism + y+) → parallel solve → results**, all driven from Rust.
- GPU-accelerated solve matching CPU accuracy.
- Mesh output functionally comparable to the reference commercial workflow.
- Linux-native, single-binary-feel app with interactive mesh/solve UI.

---

## 7. Licensing & Copyright Compliance

The full licensing and copyright-compliance rules live in
[licensing.md](licensing.md). This project uses only open-source, public-domain,
or fair-use tools, and treats proprietary CFD products as inspiration only.

---

_Last updated: 2026-11-02._
