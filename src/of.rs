//! OpenFOAM orchestration — run meshing/solver tools as subprocesses.
//!
//! OpenFOAM is treated as a CLI library: Fluidcraft generates case files and
//! invokes `blockMesh`, `snappyHexMesh`, `decomposePar`, and solvers as
//! subprocesses. No FFI.

use crate::error::FluidcraftError;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A discovered OpenFOAM installation.
#[derive(Debug, Clone)]
pub struct Openfoam {
    /// Root of the installation (where `bashrc` / tools live).
    pub root: PathBuf,
    /// Detected version string.
    pub version: String,
}

/// Locate an OpenFOAM installation and report its version.
///
/// Returns `FluidcraftError::OpenfoamMissing` when no OpenFOAM tools are on PATH.
pub fn openfoam_version() -> Result<String> {
    let output = Command::new("blockMesh")
        .arg("-help")
        .output()
        .context("failed to spawn blockMesh; is OpenFOAM loaded? (source bashrc)")?;

    if !output.status.success() {
        // blockMesh -help returns non-zero on usage; still a sign it exists.
        // Fall back to checking that the binary is resolvable.
        if Command::new("which")
            .arg("blockMesh")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Ok("blockMesh (version undetected offline)".to_string());
        }
        return Err(FluidcraftError::OpenfoamMissing.into());
    }

    Ok("blockMesh available".to_string())
}

/// Run a subprocess, capturing combined stdout/stderr.
async fn run_tool(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
) -> Result<std::process::Output> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let output = cmd
        .output()
        .with_context(|| format!("failed to spawn `{program}`"))?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        return Err(FluidcraftError::OpenfoamFailed {
            command: format!("{program} {}", args.join(" ")),
            code: output.status.code().unwrap_or(-1),
            message: message.trim().to_string(),
        }
        .into());
    }
    Ok(output)
}

/// Create a minimal OpenFOAM case directory skeleton.
///
/// Layout:
/// ```text
/// <case>/
///   0/          initial geometry + fields
///   system/     blockMeshDict, snappyHexMeshDict, controlDict, decomposeDict
///   constant/   polyMesh, transportProperties, thermophysicalProperties
/// ```
pub async fn create_case(case: &Path) -> Result<()> {
    for sub in ["0", "system", "constant"] {
        tokio::fs::create_dir_all(case.join(sub))
            .await
            .with_context(|| format!("creating {}", case.join(sub).display()))?;
    }

    write_files(case).await?;
    Ok(())
}

/// Generate the default case files.
async fn write_files(case: &Path) -> Result<()> {
    let system = case.join("system");
    let constant = case.join("constant");

    tokio::fs::write(
        system.join("blockMeshDict"),
        BLOCK_MESH_DICT,
    )
    .await?;
    tokio::fs::write(
        system.join("snappyHexMeshDict"),
        SNAPPYHEXMESH_DICT,
    )
    .await?;
    tokio::fs::write(system.join("controlDict"), CONTROL_DICT).await?;
    tokio::fs::write(
        system.join("decomposeDict"),
        DECOMPOSE_DICT,
    )
    .await?;
    tokio::fs::write(
        constant.join("transportProperties"),
        TRANSPORT_PROPERTIES,
    )
    .await?;

    Ok(())
}

/// Run `blockMesh` for the base hex mesh.
pub async fn mesh(case: &Path) -> Result<()> {
    run_tool("blockMesh", &["-case", &case.to_string_lossy()], Some(case))
        .await
        .context("blockMesh failed")?;
    Ok(())
}

/// Run `snappyHexMesh` for polyhedral snapping + prism layers.
pub async fn snappy_hex_mesh(case: &Path) -> Result<()> {
    run_tool(
        "snappyHexMesh",
        &["-case", &case.to_string_lossy(), "-overwrite"],
        Some(case),
    )
    .await
    .context("snappyHexMesh failed")?;
    Ok(())
}

/// Run `decomposePar` for parallel domain decomposition.
pub async fn decompose(case: &Path, num_domains: usize) -> Result<()> {
    run_tool(
        "decomposePar",
        &["-case", &case.to_string_lossy(), "-n", &num_domains.to_string()],
        Some(case),
    )
    .await
    .context("decomposePar failed")?;
    Ok(())
}

/// Run a solver over the case directory.
pub async fn solve(case: &Path, solver: &str) -> Result<()> {
    run_tool(
        solver,
        &["-case", &case.to_string_lossy()],
        Some(case),
    )
    .await
    .with_context(|| format!("{solver} failed"))?;
    Ok(())
}

// --- Default case file contents -------------------------------------------

const BLOCK_MESH_DICT: &str = r#"/* Fluidcraft default blockMeshDict */
FoamFile {
    version     2.0;
    format      ascii;
    class       dictionary;
    object      blockMeshDict;
}

deltaX    1e-3;
scale     1.0;

blocks
(
    hex (0 1 2 3 4 5 6 7) (10 10 10) simpleGrading (1 1 1)
);

edges
(
);

boundary
(
);

mergePatchPairs
(
);
"#;

const SNAPPYHEXMESH_DICT: &str = r#"/* Fluidcraft default snappyHexMeshDict */
FoamFile {
    version     2.0;
    format      ascii;
    class       dictionary;
    object      snappyHexMeshDict;
}

castellateControls {
    maxLocalCells 100000;
    maxTotalCells 10000000;
    maxGlobalSkewness 150;
    castellationsOnSurface 1;
}

snapControls {
    nSmoothPatches 3;
    nSmoothSurfacePoints 5;
    nSmoothPatch 5;
    tolerance 2.0;
    nSolveIter 100;
    nRelaxIter 5;
}

addLayersControls {
    relativeSizes true;
    expansionRatio 1.2;
    finalLayerThickness 0.3;
    minThickness 0.05;
    nSmoothThickness 100;
    maxThicknessIter 100;
    tolerance 0.1;
    nSmoothSurfaces 5;
}
"#;

const CONTROL_DICT: &str = r#"/* Fluidcraft default controlDict */
FoamFile {
    version     2.0;
    format      ascii;
    class       dictionary;
    object      controlDict;
}

startTime   0;
stopAt      endTime;
endTime     100;
deltaT      1;
writeInterval 10;
purgeWrite  0;
writeFormat ascii;
printTime   true;
"#;

const DECOMPOSE_DICT: &str = r#"/* Fluidcraft default decomposeDict (metis) */
FoamFile {
    version     2.0;
    format      ascii;
    class       dictionary;
    object      decomposeDict;
}

nDomains      4;
method        metis;
methodOptions {
    metis {
        rootMethod    balanced;
        balancingFactor 0.05;
    }
}
"#;

const TRANSPORT_PROPERTIES: &str = r#"/* Fluidcraft default transportProperties */
FoamFile {
    version     2.0;
    format      ascii;
    class       dictionary;
    object      transportProperties;
}

transportModel  Newtonian;
mu              1.8e-05;
rho             1.225;
"#;
