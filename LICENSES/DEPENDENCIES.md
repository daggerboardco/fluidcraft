# Third-Party Dependencies & Licenses

Fluidcraft's own code is licensed under the [MIT License](../LICENSE).

This file documents the licenses of third-party dependencies used by the
project. Every dependency must be open source, public domain, or used under
fair use. See
[`roadmap.md#7-licensing--copyright-compliance`](../roadmap.md#7-licensing--copyright-compliance)
for the binding rules.

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

> **Note:** OpenFOAM is GPL-v3. It is invoked as an external subprocess, so it
> is not statically or dynamically linked into Fluidcraft's binary. Fluidcraft's
> own source remains MIT. Confirm distribution implications with legal counsel
> before shipping.

## How to add a dependency

When adding a crate, update this table with its version and license, and confirm
it falls under an acceptable category above. Run `cargo about` (or equivalent)
before release to audit all licenses.
