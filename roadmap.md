# Fluidcraft — Project Roadmap

> A CFD application that replaces Autodesk CFD 2027's native single-threaded solver with **OpenFOAM** as the compute backend — Linux-native, parallel, and GPU-capable. Mesh creation is the core focus.

**Language:** Rust (throughout — shell, geometry, orchestration, GPU backend).
**Solver / meshing engine:** OpenFOAM (invoked as a subprocess library).
**Goal:** Recreate CFD2027's meshing workflow on Linux, swap the solver for OpenFOAM, and target GPU acceleration.

---

## 0. Background — What CFD2027 Actually Is

Reverse-engineered with REA (Reverse Engineer Anything) against the install at
`/mnt/970EVO/Autodesk/CFD 2027`. Findings that drive this project:

- **Architecture:** native C++ solver/mesher + a managed .NET 10 UI/connectivity
  layer (DevExpress WinForms + WPF). Census of the install: **106 managed** vs
  **153 native** `.dll`/`.exe`.
- **Mesher is native C++** (`ASMA*.dll` AutoMesh engine, `CFDMesher.exe`,
  `CFDGeoMesh.dll`, `CFDGeometryDatabase.dll`, `CFDFileIO.dll`, `SimMeshing.dll`).
  Proprietary — porting it is a dead end. **Do not reimplement the mesher.**
- **Geometry import supported:** STEP, IGES, Parasolid/SAT, NX/UG, CATIA,
  ProE/CREO, SolidWorks, plus STL, OBJ, DXF, JT.
- **Meshing recipe extracted (from native-DLL strings):**
  - Algorithm: **octree-dominant** + **delaunay** + **advancing-front/"bubble"**,
    with **polyhedral conversion** and **non-conformal interface matching**.
  - Elements: **tetrahedral core**; **wedge/prism/pyramid** for near-wall; shells.
  - Boundary layers: **inflation layers** with **y+ targeting**, **wall functions**,
    **k-ω SST** turbulence.
  - Sizing: global/local **sizing**, **growth rate**, **transition**, **AMR**
    (adaptive refinement), quality/skewness control.
  - Parallel: **MPI** + **metis domain decomposition** + partitioning.
- **Key insight:** OpenFOAM already produces this exact mesh.
  `snappyHexMesh` = polyhedral core + prism inflation layers + y+;
  `blockMesh` = hex core; `decomposePar` (metis) = MPI decomposition;
  `refineMesh` = AMR. **Your IP is the app shell + geometry + GPU backend, not the meshing algorithm.**

---

## 1. Guiding Principles

1. **Don't reimplement the mesher.** Orchestrate OpenFOAM's meshers, which already
   match CFD2027's recipe.
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

### Phase 0 — Toolchain & Spike (1 week)
- Install OpenFOAM (distro build or ESI `openfoam` / `distro-clean`) + ParaView.
- Initialize the Cargo project at `/mnt/970EVO/Projects/Fluidcraft`.
- **Spike:** spawn `blockMesh` → run a solver → parse results end-to-end.
- Establish crate stack: `tokio`, `clap`, `serde`, `anyhow`, `thiserror`.
- **Definition of done:** a Rust binary runs a minimal OpenFOAM case and prints residuals.

### Phase 1 — Geometry Import (2–3 weeks) — *the risk*
- Rust's CAD ecosystem is immature. Three realistic paths:
  1. **OCC bindings** — `occt` crate (bindgen over OpenCASCADE) for true STEP/IGES/Parasolid in-Rust.
  2. **Shell out** — call FreeCAD's `importPart` or Python `cadquery` as a geometry service (fastest to working).
  3. **STL/OBJ first** — `stl` / `obj-rs` crates as a stopgap, add CAD later.
- **Recommendation:** start with option 2 or 3 to de-risk; graduate to OCC bindings once the pipeline works.
- Normalize imported geometry into a clean B-rep; add surface cleanup (small-face removal, gap/tolerance fixing).
- **Definition of done:** import a STEP file and produce a clean, watertight surface in-Rust.

### Phase 2 — Meshing (3–4 weeks) — *core focus*
- Orchestrate OpenFOAM CLIs from Rust:
  - `blockMesh` for hex cores / simple domains.
  - `snappyHexMesh` for the CFD2027-equivalent recipe: **snap → smooth → add prism layers → y+ control**.
- Generate the `snappyHexMeshDict` from Rust (snap/smooth/prism-layer/y+ settings).
- Add local sizing + refinement boxes; run `checkMesh` for quality/skewness/orthogonality.
- Validate mesh against CFD2027 output (element mix: polyhedral core + prism walls).
- **Definition of done:** produce a polyhedral + prism-layer mesh with y+ control, passing `checkMesh`.

### Phase 3 — Solver Orchestration (2–3 weeks)
- Wrap OpenFOAM solvers (`simpleFoam`, `kSSTFoam`, etc.) as subprocesses.
- Build case setup: boundary conditions, materials, turbulence models.
- Parallel: `decomposePar` (metis) + `mpirun`/`mpmd` + `recombinePar` — matches CFD2027's MPI partitioning.
  - Use the `mpi` crate for in-process MPI control if desired.
- Job monitoring: parse `log.*`, residuals, y+, forces.
- **Definition of done:** run a parallel OpenFOAM solve and report converged residuals + y+.

### Phase 4 — GPU Backend (3–6 weeks) — *the hard part*
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
- Mesh output functionally comparable to CFD2027's recipe.
- Linux-native, single-binary-feel app with interactive mesh/solve UI.

---

_Last updated: 2026-11-02. Source of CFD2027 findings: REA static inspection of `/mnt/970EVO/Autodesk/CFD 2027`._
