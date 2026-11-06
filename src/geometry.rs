//! Geometry import for Fluidcraft.
//!
//! Converts CAD STEP models to watertight triangulated surfaces (STL) using the
//! solidcore-backed `solidcraft` CLI — the SMLib (NVIDIA usd-brep) Rust-port
//! kernel. The output is normalized to a clean ASCII STL so OpenFOAM's
//! `triSurface` parser reads it deterministically regardless of the upstream
//! exporter's format or header wording.
//!
//! This is the Phase 1 geometry-import path. The longer-term goal is to link the
//! `solidcore` Rust crate directly; until that crate is published and stable
//! (currently experimental 0.0.x), we shell out to the `solidcraft-cli` binary
//! that embeds the same kernel.

use crate::error::FluidcraftError;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A single STL triangle with its normal and three vertices.
#[derive(Debug, Clone, PartialEq)]
pub struct StlTriangle {
    pub normal: [f32; 3],
    pub v0: [f32; 3],
    pub v1: [f32; 3],
    pub v2: [f32; 3],
}

/// A triangulated surface.
#[derive(Debug, Clone, Default)]
pub struct StlMesh {
    pub triangles: Vec<StlTriangle>,
}

impl StlMesh {
    /// Axis-aligned bounding box as `(min_xyz, max_xyz)`.
    pub fn bounding_box(&self) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut any = false;
        for t in &self.triangles {
            for v in [t.v0, t.v1, t.v2] {
                any = true;
                for i in 0..3 {
                    if v[i] < min[i] {
                        min[i] = v[i];
                    }
                    if v[i] > max[i] {
                        max[i] = v[i];
                    }
                }
            }
        }
        if !any {
            return ([0.0; 3], [0.0; 3]);
        }
        (min, max)
    }

    /// Edge length of the bounding box's largest dimension.
    pub fn max_extent(&self) -> f32 {
        let (min, max) = self.bounding_box();
        (max[0] - min[0]).max(max[1] - min[1]).max(max[2] - min[2])
    }
}

/// Read an STL file, auto-detecting binary vs ASCII.
///
/// Handles three real-world layouts:
/// - ASCII STL (`solid` header + `facet`/`vertex` text)
/// - Standard binary STL (80-byte header + u32 count + triangles)
/// - solidcraft's hybrid: 80-byte text header + binary STL body
pub fn read_stl(path: &Path) -> Result<StlMesh> {
    let bytes =
        std::fs::read(path).with_context(|| format!("reading STL file {}", path.display()))?;

    // ASCII STL is text: identify it by the `facet` keyword, which appears right
    // after the optional `solid` header. Binary bodies (including solidcraft's
    // 80-byte-header variant) won't contain that token in their first bytes.
    let head = &bytes[..bytes.len().min(512)];
    if contains_token(head, b"facet") {
        return read_stl_ascii(&bytes);
    }

    // Binary STL: 80-byte header, u32 (LE) triangle count, then 50 bytes/triangle.
    read_stl_binary(&bytes)
}

fn contains_token(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Parse an STL numeric token, returning a clear error on malformed input.
///
/// Silently substituting zero (the previous behaviour) could corrupt a vertex
/// or normal without warning; failing loudly keeps a malformed surface from
/// silently producing a wrong mesh (audit 2026-11-05).
fn parse_stl_f32(tok: &str) -> Result<f32> {
    tok.parse::<f32>()
        .with_context(|| format!("invalid number in STL token: '{tok}'"))
}

fn read_stl_ascii(bytes: &[u8]) -> Result<StlMesh> {
    let text = String::from_utf8(bytes.to_vec()).context("ASCII STL is not valid UTF-8")?;
    let mut mesh = StlMesh::default();
    let mut i = 0;
    let tokens: Vec<&str> = text.split_whitespace().collect();
    while i < tokens.len() {
        let tok = tokens[i];
        if tok == "facet" {
            // facet normal nx ny nz
            // outer loop
            //   vertex x y z (x3)
            // endloop
            // endfacet
            let mut normal = [0.0f32; 3];
            let mut verts = Vec::new();
            i += 1; // "facet"
                    // ASCII STL is `facet normal nx ny nz`: skip the literal "normal"
                    // keyword before its three components. Reading the keyword as a
                    // component silently zeroed every normal and dropped a coordinate
                    // (audit 2026-11-05).
            if i >= tokens.len() || tokens[i] != "normal" {
                return Err(anyhow::anyhow!(
                    "malformed ASCII STL: facet missing 'normal' keyword"
                ));
            }
            i += 1; // "normal"
            if i + 3 > tokens.len() {
                return Err(anyhow::anyhow!(
                    "malformed ASCII STL: facet with no normal components"
                ));
            }
            normal[0] = parse_stl_f32(tokens[i])?;
            normal[1] = parse_stl_f32(tokens[i + 1])?;
            normal[2] = parse_stl_f32(tokens[i + 2])?;
            i += 3;
            // skip "outer" "loop"
            while i < tokens.len() && tokens[i] != "vertex" {
                i += 1;
            }
            while i < tokens.len() && tokens[i] == "vertex" {
                // Reads tokens[i+1..=i+3]; the guard must admit the last index,
                // so error when i + 3 >= len. The prior `>` off-by-one let a
                // truncated final vertex index out of bounds and panic.
                if i + 3 >= tokens.len() {
                    return Err(anyhow::anyhow!(
                        "malformed ASCII STL: vertex with incomplete coordinates"
                    ));
                }
                let v = [
                    parse_stl_f32(tokens[i + 1])?,
                    parse_stl_f32(tokens[i + 2])?,
                    parse_stl_f32(tokens[i + 3])?,
                ];
                verts.push(v);
                i += 4;
            }
            if verts.len() < 3 {
                return Err(anyhow::anyhow!(
                    "malformed ASCII STL: facet needs 3 vertices"
                ));
            }
            mesh.triangles.push(StlTriangle {
                normal,
                v0: verts[0],
                v1: verts[1],
                v2: verts[2],
            });
        } else {
            i += 1;
        }
    }
    Ok(mesh)
}

fn read_stl_binary(bytes: &[u8]) -> Result<StlMesh> {
    if bytes.len() < 84 {
        return Err(anyhow::anyhow!("binary STL too short"));
    }
    let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    let mut mesh = StlMesh::default();
    let mut off = 84;
    for _ in 0..n {
        if off + 50 > bytes.len() {
            return Err(anyhow::anyhow!(
                "binary STL truncated ({} triangles declared)",
                n
            ));
        }
        let f32_at = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let normal = [f32_at(off), f32_at(off + 4), f32_at(off + 8)];
        let v0 = [f32_at(off + 12), f32_at(off + 16), f32_at(off + 20)];
        let v1 = [f32_at(off + 24), f32_at(off + 28), f32_at(off + 32)];
        let v2 = [f32_at(off + 36), f32_at(off + 40), f32_at(off + 44)];
        mesh.triangles.push(StlTriangle { normal, v0, v1, v2 });
        off += 50; // 12-byte normal + 36-byte vertices + 2-byte attribute
    }
    Ok(mesh)
}

/// Write a clean ASCII STL. When `scale` is non-zero, every vertex is multiplied
/// by it (used to normalize mm-exported geometry into meters, etc.).
pub fn write_stl_ascii(path: &Path, mesh: &StlMesh, scale: f32) -> Result<()> {
    let mut out = String::new();
    out.push_str("solid fluidcraft\n");
    for t in &mesh.triangles {
        let n = if scale != 0.0 {
            [
                t.normal[0] * scale,
                t.normal[1] * scale,
                t.normal[2] * scale,
            ]
        } else {
            t.normal
        };
        out.push_str(&format!(
            "facet normal {:.9e} {:.9e} {:.9e}\n",
            n[0], n[1], n[2]
        ));
        out.push_str("  outer loop\n");
        for v in [t.v0, t.v1, t.v2] {
            let p = if scale != 0.0 {
                [v[0] * scale, v[1] * scale, v[2] * scale]
            } else {
                v
            };
            out.push_str(&format!(
                "    vertex {:.9e} {:.9e} {:.9e}\n",
                p[0], p[1], p[2]
            ));
        }
        out.push_str("  endloop\nendfacet\n");
    }
    out.push_str("endsolid fluidcraft\n");
    std::fs::write(path, out).with_context(|| format!("writing STL file {}", path.display()))?;
    Ok(())
}

/// Locate the `solidcraft-cli` binary.
///
/// Resolution order: explicit path, then `$FLUIDCRAFT_SOLIDCRAFT`, then
/// `$PATH`. Returns `FluidcraftError::SolidcraftMissing` when none is found.
pub fn locate_solidcraft(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        return Err(FluidcraftError::Geometry(format!(
            "solidcraft binary not found at {}",
            p.display()
        ))
        .into());
    }
    if let Ok(env) = std::env::var("FLUIDCRAFT_SOLIDCRAFT") {
        let p = PathBuf::from(&env);
        if p.exists() {
            return Ok(p);
        }
    }
    // Search PATH.
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("solidcraft-cli");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(FluidcraftError::Geometry(
        "solidcraft-cli not found; set --solidcraft or $FLUIDCRAFT_SOLIDCRAFT".to_string(),
    )
    .into())
}

/// Convert a STEP file to a normalized ASCII STL via the solidcraft CLI.
///
/// `scale` normalizes units (1.0 = leave as-is; 0.001 = mm->m). `output` is the
/// final normalized STL. A temporary STL is produced by solidcraft and then
/// normalized to ASCII in `output`.
pub fn step_to_stl(input: &Path, output: &Path, solidcraft_bin: &Path, scale: f32) -> Result<()> {
    if !input.exists() {
        return Err(FluidcraftError::Geometry(format!(
            "input STEP file not found: {}",
            input.display()
        ))
        .into());
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    // solidcraft writes its own STL next to the requested output; read that back
    // and normalize into `output`.
    let tmp = output.with_extension("stl.tmp").with_extension("stl");
    let status = Command::new(solidcraft_bin)
        .arg("run")
        .arg(input)
        .arg("--out")
        .arg(&tmp)
        .output()
        .with_context(|| format!("failed to spawn `{}`", solidcraft_bin.display()))?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(FluidcraftError::Geometry(format!(
            "solidcraft failed to convert STEP (exit {}): {}",
            status.status.code().unwrap_or(-1),
            stderr.trim()
        ))
        .into());
    }

    let mesh = read_stl(&tmp)?;
    write_stl_ascii(output, &mesh, scale)?;

    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_mesh() -> StlMesh {
        StlMesh {
            triangles: vec![StlTriangle {
                normal: [0.0, 0.0, 1.0],
                v0: [0.0, 0.0, 0.0],
                v1: [1.0, 0.0, 0.0],
                v2: [0.0, 1.0, 0.0],
            }],
        }
    }

    #[test]
    fn ascii_roundtrip_preserves_triangle() {
        let dir = std::env::temp_dir().join(format!("fc-stl-ascii-{}", std::process::id()));
        let path = dir.join("tri.stl");
        std::fs::create_dir_all(&dir).unwrap();
        write_stl_ascii(&path, &sample_mesh(), 1.0).unwrap();

        let mesh = read_stl(&path).unwrap();
        assert_eq!(mesh.triangles.len(), 1);
        assert_eq!(mesh.triangles[0].v1, [1.0, 0.0, 0.0]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binary_stl_reads_correctly() {
        // Build a minimal 1-triangle binary STL in memory.
        let mut bytes: Vec<u8> = vec![0u8; 80];
        bytes.extend_from_slice(&1u32.to_le_bytes());
        let tri = |normal: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]| -> Vec<u8> {
            let mut v = Vec::new();
            for f in normal {
                v.extend_from_slice(&f.to_le_bytes());
            }
            for p in [a, b, c] {
                for f in p {
                    v.extend_from_slice(&f.to_le_bytes());
                }
            }
            v.extend_from_slice(&0u16.to_le_bytes());
            v
        };
        bytes.extend_from_slice(&tri(
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        ));
        let mesh = read_stl_binary(&bytes).unwrap();
        assert_eq!(mesh.triangles.len(), 1);
        assert_eq!(mesh.triangles[0].v2, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn scale_rescales_vertices() {
        let dir = std::env::temp_dir().join(format!("fc-stl-scale-{}", std::process::id()));
        let path = dir.join("scaled.stl");
        std::fs::create_dir_all(&dir).unwrap();
        write_stl_ascii(&path, &sample_mesh(), 0.001).unwrap();
        let mesh = read_stl(&path).unwrap();
        assert_eq!(mesh.triangles[0].v1, [0.001, 0.0, 0.0]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bounding_box_of_sample() {
        let mesh = sample_mesh();
        let (min, max) = mesh.bounding_box();
        assert_eq!(min, [0.0, 0.0, 0.0]);
        assert_eq!(max, [1.0, 1.0, 0.0]);
        assert!((mesh.max_extent() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn ascii_roundtrip_preserves_normal() {
        // The parser must read the literal "normal" keyword and its three
        // components. Earlier code consumed the keyword as a component, zeroing
        // every normal (audit 2026-11-05).
        let dir = std::env::temp_dir().join(format!("fc-stl-norm-{}", std::process::id()));
        let path = dir.join("tri.stl");
        std::fs::create_dir_all(&dir).unwrap();
        write_stl_ascii(&path, &sample_mesh(), 1.0).unwrap();

        let mesh = read_stl(&path).unwrap();
        assert_eq!(mesh.triangles[0].normal, [0.0, 0.0, 1.0]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ascii_stl_truncated_vertex_errors_not_panics() {
        // A truncated final vertex (missing coordinates) must return an error,
        // not panic on an out-of-bounds index.
        let text = "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0";
        let dir = std::env::temp_dir().join(format!("fc-stl-trunc-{}", std::process::id()));
        let path = dir.join("trunc.stl");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, text).unwrap();

        let res = read_stl(&path);
        assert!(res.is_err(), "truncated vertex should error, got {res:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ascii_stl_invalid_coord_errors() {
        // Invalid numeric coordinates must surface an error, not silently
        // become zero (audit 2026-11-05).
        let text = "solid x\nfacet normal 0 0 1\nouter loop\nvertex a b c\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid x\n";
        let dir = std::env::temp_dir().join(format!("fc-stl-bad-{}", std::process::id()));
        let path = dir.join("bad.stl");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, text).unwrap();

        let res = read_stl(&path);
        assert!(res.is_err(), "invalid coordinate should error, got {res:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
