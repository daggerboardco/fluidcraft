# Third-Party Dependencies & Licenses

Fluidcraft's own code is licensed under the [MIT License](../LICENSE).

This file documents the licenses of third-party dependencies used by the
project. Every dependency must be open source, public domain, or used under
fair use. The tables below are the per-dependency manifest; see
[../licensing.md](../licensing.md) for the binding rules.

## Runtime Dependencies

| Crate | Version | License | Notes |
|---|---|---|---|
| `tokio` | 1 | MIT / Apache-2.0 | Async runtime |
| `clap` | 4 | MIT / Apache-2.0 | CLI parsing |
| `serde` | 1 | MIT / Apache-2.0 / BSD-3 | Serialization |
| `serde_json` | 1 | MIT / Apache-2.0 | JSON parsing |
| `anyhow` | 1 | Apache-2.0 | Error handling |
| `thiserror` | 1 | MIT / Apache-2.0 | Error derive |

## External Tools (not bundled)

| Tool | License | Used as |
|---|---|---|
| OpenFOAM | GPL-v3 | External subprocess (no FFI, no linking) |
| ParaView | BSD-3-Clause | Post-processing (optional) |
| solidcore / solidcraft | Apache-2.0 (+ MIT components) | Geometry STEP→STL conversion via the `solidcraft-cli` binary |

> **Note:** OpenFOAM is GPL-v3. It is invoked as an external subprocess, so it
> is not statically or dynamically linked into Fluidcraft's binary. Fluidcraft's
> own source remains MIT. Confirm distribution implications with legal counsel
> before shipping.

### solidcore / solidcraft (geometry kernel)

Fluidcraft's STEP→STL geometry import ([`src/geometry.rs`](../src/geometry.rs))
shells out to the **`solidcraft-cli`** binary, which embeds the **solidcore**
kernel — a Rust port of NVIDIA's SMLib B-rep kernel (`usd-brep`).

| Component | License | Upstream | Role in Fluidcraft |
|---|---|---|---|
| solidcore (kernel) | Apache-2.0 (NVIDIA) with MIT components (`crates/ssi`) | `github.com/tangency-labs/solidcore` | B-rep geometry kernel, embedded in the CLI |
| solidcraft (CLI app) | Apache-2.0 | `github.com/tangency-labs/solidcraft` | Provides `solidcraft-cli`, invoked as a subprocess |

- **Usage:** Fluidcraft spawns `solidcraft-cli` as an external process (same
  pattern as OpenFOAM — no FFI, no linking). Discovery order: `--solidcraft`
  flag → `$FLUIDCRAFT_SOLIDCRAFT` → `$PATH`. See `src/geometry.rs`.
- **Future:** the longer-term goal is to link the `solidcore` Rust crate
  directly once it is published and stable (currently experimental 0.0.x).
- **Authorization:** tangency-labs has granted explicit permission to use any
  and all of solidcore/solidcraft's work, in addition to its permissive
  Apache-2.0/MIT licensing. See [licensing.md](licensing.md).
- **Stay-linked obligation:** to avoid an orphaned fork, these repos must be
  kept current with the `tangency-labs` upstream. See the "Upstream Sync"
  note below.

## How to add a dependency

When adding a crate, update this table with its version and license, and confirm
it falls under an acceptable category above. Run `cargo about` (or equivalent)
before release to audit all licenses.

## Upstream Sync (solidcore / solidcraft)

solidcore and solidcraft are maintained by **tangency-labs** and are consumed
here as **forks** that must stay linked to upstream:

- Upstream (`origin`): `github.com/tangency-labs/solidcore`, `github.com/tangency-labs/solidcraft`
- Local working copies: `/home/clawtemp/Projects/solidcore`, `/home/clawtemp/Projects/solidcraft`

**Why:** Fluidcraft depends on the `solidcraft-cli` binary. An un-updated fork
would drift from upstream bug fixes and license-relevant changes. We therefore
prefer to track upstream rather than diverge into an independent fork.

**Procedure (keep linked):** from each repo, run `git fetch origin` and merge
`origin/main` into the working branch, resolving conflicts before rebuilding
`solidcraft-cli`. **Current state (2026-11-06 16:40):** sync **completed** —
both forks fast-forwarded to upstream `tangency-labs` `origin/main` with 0/0
divergence (solidcore HEAD `d77c69f`, solidcraft HEAD `871014f`). Re-run the
procedure whenever upstream advances again.
