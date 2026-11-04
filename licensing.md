# Licensing & Copyright Compliance

Fluidcraft is built to never breach a third-party's copyright. This document
states the binding rules for everything the project references or uses.

## Binding rule

Any proprietary commercial CFD product may be used only as an **inspiration for
general engineering techniques** — never as a source of code, binaries, assets,
resources, or source to copy, port, decompile, or reverse engineer.

## Every dependency must fall into one of these categories

1. **Open source** under a permissive or copyleft license we comply with
   (e.g. MIT, Apache-2.0, BSD-3, GPL/LGPL with proper attribution/relinking).
2. **Public domain / copyright-free** (CC0, unlicensed public-domain data).
3. **Fair use** — only small, transformative excerpts with a documented legal
   basis; never whole files or functional substitutes.

See [`LICENSES/DEPENDENCIES.md`](LICENSES/DEPENDENCIES.md) for the per-crate and
per-tool license manifest.

## Do / Don't

| Do | Don't |
|---|---|
| Use OpenFOAM, OpenCASCADE/OCCT, FreeCAD, cadquery, VTK, Qt, WGPU, Bevy, etc. | Copy/port proprietary CFD code, meshes, geometry, or binaries |
| Follow open import standards (STEP/IGES per ISO) via open readers | Ship or bundle any proprietary file formats/readers |
| Keep a `LICENSES/` file listing every third-party dependency + license | Decompile or reverse-engineer a competitor to reproduce behavior |
| Attribute open-source components per their licenses | Reproduce copyrighted UI, icons, logos, or docs |

## OpenFOAM note

OpenFOAM is distributed under **GPL-v3** (GNU General Public License). Using it
as a subprocess library is permitted, but GPL obligations (relinking/attribution,
and considerations around combining) apply — confirm with legal counsel before
distribution. Prefer invoking it as an external process (which this architecture
already does) and keep Fluidcraft's own source under a permissive license (MIT)
where legally sound.

## Action items

- Add `LICENSES/` documenting every third-party crate/tool and its license.
- Add a top-level `LICENSE` (MIT) for Fluidcraft's own code.
- Before shipping, run a dependency-license audit (`cargo audit`, `cargo about`,
  or equivalent) and get a legal sign-off on the GPL (OpenFOAM) interaction.
