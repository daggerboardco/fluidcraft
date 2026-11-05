//! OpenFOAM orchestration — run meshing/solver tools as subprocesses.
//!
//! OpenFOAM is treated as a CLI library: Fluidcraft generates case files and
//! invokes `blockMesh`, `snappyHexMesh`, `decomposePar`, and solvers as
//! subprocesses. No FFI.

use crate::error::FluidcraftError;
use crate::geometry;
use crate::meshing;
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

/// Profile describing how to import geometry into a new case.
///
/// When present, `create_case_with_geometry` converts the STEP model to a
/// normalized ASCII STL, derives the mesh bounding box from it, and generates
/// geometry-aware `blockMeshDict` / `snappyHexMeshDict` entries that enclose
/// the part. The generated blockMesh patches (`inlet`, `outlet`, `lowerWall`,
/// `upperWall`, `sideWalls`) match the baseline field BCs, so cases stay
/// consistent whether or not geometry is imported.
#[derive(Debug, Clone)]
pub struct GeometryProfile {
    /// Input STEP file path.
    pub input: PathBuf,
    /// Unit scale applied to the STL (1.0 = as-is, 0.001 = mm -> m).
    pub scale: f32,
    /// Optional explicit solidcraft binary path (else $FLUIDCRAFT_SOLIDCRAFT / PATH).
    pub solidcraft: Option<PathBuf>,
    /// Enable prism boundary-layer inflation layers.
    pub add_layers: bool,
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

/// Resolve a case path to an absolute path so that OpenFOAM tool invocations
/// are unambiguous regardless of the process working directory. Using an
/// absolute path for both the working directory and the `-case` argument keeps
/// them consistent (a relative `-case` combined with `chdir` would otherwise
/// resolve to the wrong location).
fn resolve_case(case: &Path) -> PathBuf {
    if case.is_absolute() {
        case.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|dir| dir.join(case))
            .unwrap_or_else(|_| case.to_path_buf())
    }
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

/// Create a minimal OpenFOAM case directory skeleton with an analytical box.
///
/// Layout:
/// ```text
/// <case>/
///   0/          initial geometry + fields
///   system/     blockMeshDict, snappyHexMeshDict, controlDict, decomposeDict
///   constant/   polyMesh, transportProperties, thermophysicalProperties
/// ```
pub async fn create_case(case: &Path, overwrite: bool) -> Result<()> {
    create_case_with_geometry(case, overwrite, None).await
}

/// Create a minimal OpenFOAM case directory skeleton, importing geometry when
/// provided.
///
/// With `geometry`, the STEP model is converted to an ASCII STL under
/// `constant/geometry/`, the mesh bounding box is derived from it, and
/// geometry-aware `blockMeshDict` / `snappyHexMeshDict` are generated to enclose
/// the part. Without it, the analytical-box baseline templates are used.
pub async fn create_case_with_geometry(
    case: &Path,
    overwrite: bool,
    geometry: Option<GeometryProfile>,
) -> Result<()> {
    let resolved = resolve_case(case);

    if !overwrite && case_exists(&resolved).await? {
        return Err(FluidcraftError::CaseExists {
            path: resolved.to_string_lossy().into_owned(),
        }
        .into());
    }

    for sub in ["0", "system", "constant"] {
        tokio::fs::create_dir_all(resolved.join(sub))
            .await
            .with_context(|| format!("creating {}", resolved.join(sub).display()))?;
    }

    write_files(&resolved, geometry.as_ref()).await?;
    Ok(())
}

/// Returns true when the case directory already contains at least one entry,
/// i.e. it is an existing case rather than a fresh empty target.
async fn case_exists(case: &Path) -> Result<bool> {
    let mut reader = match tokio::fs::read_dir(case).await {
        Ok(reader) => reader,
        // A missing target is a fresh case, not an existing one.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("reading {}", case.display())),
    };
    Ok(reader.next_entry().await?.is_some())
}

/// Generate the default case files.
///
/// When `geometry` is provided, the STEP model is converted to an ASCII STL and
/// the mesh dictionaries are generated to enclose it; otherwise the analytical
/// box baseline templates are written.
async fn write_files(case: &Path, geometry: Option<&GeometryProfile>) -> Result<()> {
    let system = case.join("system");
    let constant = case.join("constant");

    let (block_mesh_dict, snappy_dict) = match geometry {
        Some(profile) => generate_geometry_dicts(case, profile).await?,
        None => (BLOCK_MESH_DICT.to_string(), SNAPPYHEXMESH_DICT.to_string()),
    };

    tokio::fs::write(system.join("blockMeshDict"), block_mesh_dict).await?;
    tokio::fs::write(system.join("snappyHexMeshDict"), snappy_dict).await?;
    tokio::fs::write(system.join("controlDict"), CONTROL_DICT).await?;
    tokio::fs::write(system.join("decomposeDict"), DECOMPOSE_DICT).await?;
    tokio::fs::write(system.join("fvSchemes"), FV_SCHEMES).await?;
    tokio::fs::write(system.join("fvSolution"), FV_SOLUTION).await?;

    // Initial + boundary-condition fields. When geometry is imported, the
    // snappy generator creates a wall patch for the imported triSurface, so the
    // fields must define a matching BC for that patch (audit 2026-11-05).
    let stem = geometry
        .and_then(|g| g.input.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "part".to_string());
    let has_geometry = geometry.is_some();

    let u = if has_geometry {
        with_surface_bc(FIELD_U, &surface_wall_bc(&stem, "noSlip", None))
    } else {
        FIELD_U.to_string()
    };
    let p = if has_geometry {
        with_surface_bc(FIELD_P, &surface_wall_bc(&stem, "zeroGradient", None))
    } else {
        FIELD_P.to_string()
    };
    let nut = if has_geometry {
        with_surface_bc(FIELD_NUT, &surface_wall_bc(&stem, "zeroGradient", None))
    } else {
        FIELD_NUT.to_string()
    };
    let nu_tilda = if has_geometry {
        with_surface_bc(FIELD_NUTTILDA, &surface_wall_bc(&stem, "fixedValue", Some("uniform 1e-05")))
    } else {
        FIELD_NUTTILDA.to_string()
    };

    tokio::fs::write(case.join("0").join("U"), u).await?;
    tokio::fs::write(case.join("0").join("p"), p).await?;
    tokio::fs::write(case.join("0").join("nut"), nut).await?;
    tokio::fs::write(case.join("0").join("nuTilda"), nu_tilda).await?;
    tokio::fs::write(constant.join("transportProperties"), TRANSPORT_PROPERTIES).await?;
    tokio::fs::write(constant.join("momentumTransport"), MOMENTUM_TRANSPORT).await?;

    Ok(())
}

/// Convert an imported STEP model to a normalized ASCII STL and generate the
/// mesh dictionaries that enclose it.
///
/// Returns `(blockMeshDict, snappyHexMeshDict)`. The STL is written to
/// `constant/geometry/<stem>.stl`, and the blockMesh box is sized with a margin
/// equal to half the part's largest dimension so snapping and boundary layers
/// have room to develop.
async fn generate_geometry_dicts(
    case: &Path,
    profile: &GeometryProfile,
) -> Result<(String, String)> {
    let geometry_dir = case.join("constant").join("geometry");
    tokio::fs::create_dir_all(&geometry_dir)
        .await
        .with_context(|| format!("creating {}", geometry_dir.display()))?;

    let stem = profile
        .input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "part".to_string());
    let surface_file = format!("{}.stl", stem);
    let stl_path = geometry_dir.join(&surface_file);

    // Locate the solidcraft binary and convert STEP -> STL -> bounding box.
    let bbox = {
        let bin = geometry::locate_solidcraft(profile.solidcraft.as_deref())?;
        geometry::step_to_stl(&profile.input, &stl_path, &bin, profile.scale)?;
        let mesh = geometry::read_stl(&stl_path)?;
        let (min, max) = mesh.bounding_box();
        meshing::BoundingBox {
            min: [min[0] as f64, min[1] as f64, min[2] as f64],
            max: [max[0] as f64, max[1] as f64, max[2] as f64],
        }
    };
    let _ = case;

    let margin = 0.5 * bbox.max_extent();
    let config = meshing::MeshingConfig {
        geometry: bbox,
        box_margin: margin,
        add_layers: profile.add_layers,
        ..Default::default()
    };

    let block_mesh_dict = meshing::generate_block_mesh_dict(&bbox, margin);
    let snappy_dict = meshing::generate_snappy_hex_mesh_dict(&config, &stem, &surface_file);

    Ok((block_mesh_dict, snappy_dict))
}

/// Run `blockMesh` for the base hex mesh, then `snappyHexMesh` for
/// polyhedral snapping and prism inflation layers — the advertised pipeline.
pub async fn mesh(case: &Path) -> Result<()> {
    let case = resolve_case(case);
    run_tool(
        "blockMesh",
        &["-case", &case.to_string_lossy()],
        Some(&case),
    )
    .await
    .context("blockMesh failed")?;
    snappy_hex_mesh(&case).await?;
    Ok(())
}

/// Run `snappyHexMesh` for polyhedral snapping + prism layers.
pub async fn snappy_hex_mesh(case: &Path) -> Result<()> {
    let case = resolve_case(case);
    run_tool(
        "snappyHexMesh",
        &["-case", &case.to_string_lossy(), "-overwrite"],
        Some(&case),
    )
    .await
    .context("snappyHexMesh failed")?;
    Ok(())
}

/// Run `decomposePar` for parallel domain decomposition.
pub async fn decompose(case: &Path, num_domains: usize) -> Result<()> {
    let case = resolve_case(case);
    run_tool(
        "decomposePar",
        &[
            "-case",
            &case.to_string_lossy(),
            "-n",
            &num_domains.to_string(),
        ],
        Some(&case),
    )
    .await
    .context("decomposePar failed")?;
    Ok(())
}

/// Run a solver over the case directory.
pub async fn solve(case: &Path, solver: &str) -> Result<()> {
    let case = resolve_case(case);
    run_tool(solver, &["-case", &case.to_string_lossy()], Some(&case))
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

vertices
(
    (0 0 0)
    (1 0 0)
    (1 1 0)
    (0 1 0)
    (0 0 1)
    (1 0 1)
    (1 1 1)
    (0 1 1)
);

blocks
(
    hex (0 1 2 3 4 5 6 7) (10 10 10) simpleGrading (1 1 1)
);

edges
(
);

boundary
(
    inlet
    {
        type            patch;
        faces           ((0 4 7 3));
    }
    outlet
    {
        type            patch;
        faces           ((1 2 6 5));
    }
    lowerWall
    {
        type            patch;
        faces           ((0 1 5 4));
    }
    upperWall
    {
        type            patch;
        faces           ((3 7 6 2));
    }
    sideWalls
    {
        type            patch;
        faces
        (
            ( 0 3 2 1 )
            ( 4 5 6 7 )
        );
    }
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

// Which steps to run for the baseline.
castellatedMesh true;
snap            true;
addLayers       false;

// Geometry. For the baseline we use an analytical `box` so the generated case
// runs with no external surface file. Once real geometry import exists, replace
// with named triSurface entries (STL/OBJ).
geometry
{
    refinementBox
    {
        type box;
        min (0 0 0);
        max (1 1 1);
    }
};

// Settings for the castellatedMesh generation.
castellatedMeshControls
{
    maxLocalCells 100000;
    maxGlobalCells 7000000;
    minRefinementCells 10;
    maxLoadUnbalance 0.10;
    nCellsBetweenLevels 3;

    features
    (
    );

    refinementSurfaces
    {
    }

    resolveFeatureAngle 60;

    refinementRegions
    {
        refinementBox
        {
            mode    inside;
            level   1;
        }
    }

    insidePoint (0.5 0.5 0.5);

    allowFreeStandingZoneFaces false;
}

// Settings for the snapping.
snapControls
{
    nSmoothPatch 3;
    tolerance 4.0;
    nSolveIter 30;
    nRelaxIter 5;
}

// Settings for the layer addition (disabled in baseline).
addLayersControls
{
    relativeSizes true;

    layers
    {
    }

    expansionRatio 1.0;
    finalLayerThickness 0.3;
    minThickness 0.1;
    nGrow 0;
    featureAngle 100;
    slipFeatureAngle 30;
    nRelaxIter 3;
    nSmoothSurfaceNormals 1;
    nSmoothNormals 3;
    nSmoothThickness 2;
    maxFaceThicknessRatio 0.5;
    maxThicknessToMedialRatio 0.3;
    minMedianAxisAngle 90;
    nBufferCellsNoExtrude 0;
    nLayerIter 50;
}

// Generic mesh quality settings.
meshQualityControls
{
    maxNonOrtho 65;
    maxBoundarySkewness 20;
    maxInternalSkewness 4;
    maxConcave 80;
    minFlatness 0.5;
    minVol -1e30;
    minTetQuality 1e-30;
    minTwist 0.02;
    minDeterminant 0.001;
    minFaceWeight 0.02;
    minVolRatio 0.01;

    nSmoothScale 4;
    errorReduction 0.75;
}

// Merge tolerance. Fraction of the overall bounding box of the initial mesh.
mergeTolerance 1e-6;
"#;

const CONTROL_DICT: &str = r#"/* Fluidcraft default controlDict */
FoamFile {
    version     2.0;
    format      ascii;
    class       dictionary;
    object      controlDict;
}

solver          incompressibleFluid;

startFrom       latestTime;

startTime       0;

stopAt          endTime;

endTime         0.03;

deltaT          0.0001;

writeControl    adjustableRunTime;

writeInterval   0.001;

purgeWrite      0;

writeFormat     ascii;

writePrecision  6;

writeCompression off;

timeFormat      general;

timePrecision   6;

runTimeModifiable yes;

adjustTimeStep  yes;

maxCo           5;
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
viscosityModel  constant;
mu              1.8e-05;
nu              1.4e-05;
rho             1.225;
"#;

// OpenFOAM-14 momentum model selection. Laminar is the minimal baseline that
// needs no turbulence fields (nut/nuTilda). Once real geometry import exists,
// swap to a RAS model (e.g. kEpsilon) with `turbulence on` and add the
// corresponding initial fields.
const MOMENTUM_TRANSPORT: &str = r#"/* Fluidcraft default momentumTransport */
FoamFile {
    version     2.0;
    format      ascii;
    class       dictionary;
    location    "constant";
    object      momentumTransport;
}

simulationType laminar;
"#;

// Steady-state laminar channel: fvSchemes (discretisation).
const FV_SCHEMES: &str = r#"/* Fluidcraft default fvSchemes */
FoamFile {
    version     2.0;
    format      ascii;
    class       dictionary;
    object      fvSchemes;
}

ddtSchemes
{
    default     steadyState;
};

gradSchemes
{
    default         Gauss linear;
    grad(U)         cellLimited Gauss linear 1;
};

divSchemes
{
    default         none;
    div(phi,U)      Gauss upwind;
};

laplacianSchemes
{
    default         Gauss linear corrected;
};

interpolationSchemes
{
    default       linear;
};

snGradSchemes
{
    default       corrected;
};

fluxRequired
{
    default       no;
    p             no;
};
"#;

// Steady-state laminar channel: fvSolution (SIMPLE + linear solvers).
const FV_SOLUTION: &str = r#"/* Fluidcraft default fvSolution (SIMPLE) */
FoamFile {
    version     2.0;
    format      ascii;
    class       dictionary;
    object      fvSolution;
}

solvers
{
    p
    {
        solver           GAMG;
        smoother         DICGaussSeidel;
        tolerance        1e-7;
        relTol           0.01;
    }

    pFinal
    {
        $p;
        relTol          0;
    }

    "(U)"
    {
        solver          PBiCGStab;
        preconditioner  DILU;
        tolerance       1e-05;
        relTol          0.1;
    }

    "(U)Final"
    {
        $U;
        relTol          0;
    }
};

SIMPLE
{
    nCorrectors             2;
    nNonOrthogonalCorrectors 0;

    pRefCell                  0;
    pRefValue                 0;

    residualControl
    {
        p                     1e-3;
        U                     1e-3;
    }
};
"#;

// Initial + boundary-condition field U (laminar inflow, no-slip walls).
const FIELD_U: &str = r#"/* Fluidcraft default U */
FoamFile {
    version     2.0;
    format      ascii;
    class       volVectorField;
    object      U;
}

dimensions      [0 1 -1 0 0 0 0];

internalField   uniform (0.1 0 0);

boundaryField
{
    inlet
    {
        type            fixedValue;
        value           uniform (0.1 0 0);
    }
    outlet
    {
        type            zeroGradient;
    }
    lowerWall
    {
        type            noSlip;
    }
    upperWall
    {
        type            noSlip;
    }
    sideWalls
    {
        type            noSlip;
    }
};
"#;

// Initial + boundary-condition field p (reference at outlet).
const FIELD_P: &str = r#"/* Fluidcraft default p */
FoamFile {
    version     2.0;
    format      ascii;
    class       volScalarField;
    object      p;
}

dimensions      [0 2 -2 0 0 0 0];

internalField   uniform 0;

boundaryField
{
    inlet
    {
        type            zeroGradient;
    }
    outlet
    {
        type            fixedValue;
        value           uniform 0;
    }
    lowerWall
    {
        type            zeroGradient;
    }
    upperWall
    {
        type            zeroGradient;
    }
    sideWalls
    {
        type            zeroGradient;
    }
};
"#;

// Initial + boundary-condition field nut (SpalartAllmaras working variable).
// Required by the RAS momentumTransport model even when turbulence is off.
const FIELD_NUT: &str = r#"/* Fluidcraft default nut */
FoamFile {
    version     2.0;
    format      ascii;
    class       volScalarField;
    object      nut;
}

dimensions      [0 2 -1 0 0 0 0];

internalField   uniform 0;

boundaryField
{
    inlet
    {
        type            zeroGradient;
    }
    outlet
    {
        type            zeroGradient;
    }
    lowerWall
    {
        type            zeroGradient;
    }
    upperWall
    {
        type            zeroGradient;
    }
    sideWalls
    {
        type            zeroGradient;
    }
};
"#;

// Initial + boundary-condition field nuTilda (SpalartAllmaras transport var).
// Required alongside nut by the RAS momentumTransport model.
const FIELD_NUTTILDA: &str = r#"/* Fluidcraft default nuTilda */
FoamFile {
    version     2.0;
    format      ascii;
    class       volScalarField;
    object      nuTilda;
}

dimensions      [0 2 -1 0 0 0 0];

internalField   uniform 0;

boundaryField
{
    inlet
    {
        type            fixedValue;
        value           uniform 1e-05;
    }
    outlet
    {
        type            zeroGradient;
    }
    lowerWall
    {
        type            fixedValue;
        value           uniform 1e-05;
    }
    upperWall
    {
        type            fixedValue;
        value           uniform 1e-05;
    }
    sideWalls
    {
        type            fixedValue;
        value           uniform 1e-05;
    }
};
"#;

/// Build a wall boundary-condition block for the imported-surface patch that
/// snappyHexMesh creates from the triSurface. `value` is optional (None for
/// noSlip / zeroGradient).
fn surface_wall_bc(name: &str, bc_type: &str, value: Option<&str>) -> String {
    match value {
        Some(v) => format!(
            "    {name}\n    {{\n        type            {bt};\n        value           {val};\n    }}\n",
            name = name,
            bt = bc_type,
            val = v
        ),
        None => format!(
            "    {name}\n    {{\n        type            {bt};\n    }}\n",
            name = name,
            bt = bc_type
        ),
    }
}

/// Inject a boundary-condition block into a field template's `boundaryField`,
/// right after the `boundaryField {` opening. Used to add the imported-surface
/// wall patch to a geometry case. Brace balance and the patch-set parser are
/// preserved because exactly one `{` and one `}` are inserted.
fn with_surface_bc(base: &str, entry: &str) -> String {
    const MARKER: &str = "boundaryField\n{";
    match base.find(MARKER) {
        Some(pos) => {
            let open = pos + MARKER.len();
            let mut out = String::with_capacity(base.len() + entry.len());
            out.push_str(&base[..open]);
            out.push_str(entry);
            out.push_str(&base[open..]);
            out
        }
        None => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn with_surface_bc_injects_patch_and_keeps_balance() {
        let injected = with_surface_bc(FIELD_U, &surface_wall_bc("washer", "noSlip", None));
        assert_eq!(
            injected.matches('{').count(),
            injected.matches('}').count(),
            "brace balance must hold after injection"
        );
        assert!(injected.contains("boundaryField"));
        assert!(injected.contains("washer"));
        assert!(injected.contains("noSlip"));
        // Original box patches survive.
        for patch in ["inlet", "outlet", "lowerWall", "upperWall", "sideWalls"] {
            assert!(injected.contains(patch), "lost box patch {patch}");
        }
    }

    #[test]
    fn resolve_case_returns_absolute_path() {
        // A relative input is resolved against the current directory into an
        // absolute path, so `-case` is unambiguous regardless of cwd.
        let resolved = resolve_case(Path::new("my-case"));
        assert!(
            resolved.is_absolute(),
            "expected absolute path, got {}",
            resolved.display()
        );
    }

    #[test]
    fn resolve_case_passthrough_for_absolute() {
        let abs = std::env::current_dir().unwrap().join("other");
        assert_eq!(resolve_case(&abs), abs);
    }

    #[tokio::test]
    async fn create_case_rejects_existing_non_empty_case() {
        let dir = std::env::temp_dir().join(format!("oc-test-exist-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        // Seed an existing file so the case is non-empty.
        tokio::fs::create_dir_all(dir.join("system")).await.unwrap();
        tokio::fs::write(dir.join("system").join("seed.txt"), b"keep")
            .await
            .unwrap();

        let err = create_case(&dir, false).await;
        assert!(err.is_err(), "expected rejection of existing case");
        if let Err(e) = err {
            let msg = e.to_string();
            assert!(
                msg.contains("--overwrite"),
                "expected overwrite hint, got: {msg}"
            );
        }

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn create_case_overwrites_when_flagged() {
        let dir = std::env::temp_dir().join(format!("oc-test-over-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::create_dir_all(dir.join("system")).await.unwrap();
        tokio::fs::write(dir.join("system").join("seed.txt"), b"keep")
            .await
            .unwrap();

        let result = create_case(&dir, true).await;
        assert!(
            result.is_ok(),
            "expected success with overwrite: {result:?}"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn create_case_in_empty_dir_succeeds() {
        let dir = std::env::temp_dir().join(format!("oc-test-empty-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        // Create an existing but empty target directory.
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let result = create_case(&dir, false).await;
        assert!(result.is_ok(), "expected success in empty dir: {result:?}");
        // The generated case must be a complete, runnable baseline: meshing
        // dictionaries, solver control (fvSchemes/fvSolution), and initial
        // field files with boundary conditions.
        assert!(dir.join("system").join("blockMeshDict").exists());
        assert!(dir.join("system").join("fvSchemes").exists());
        assert!(dir.join("system").join("fvSolution").exists());
        assert!(dir.join("0").join("U").exists());
        assert!(dir.join("0").join("p").exists());
        // Every patch defined in blockMeshDict must have matching BCs in U.
        let u_text = std::fs::read_to_string(dir.join("0").join("U")).unwrap_or_default();
        for patch in ["inlet", "outlet", "lowerWall", "upperWall", "sideWalls"] {
            assert!(
                u_text.contains(patch),
                "U boundary conditions must define patch '{patch}'"
            );
        }

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    // Extract the ordered list of patch names declared in a dictionary's
    // `boundaryField` section. A patch name is a bare identifier line
    // immediately followed by a `{` line. The section keyword itself (its
    // opening brace) is skipped, and scanning stops at the section's
    // top-level closing bracket.
    fn patch_names(text: &str, section_kw: &str) -> Vec<String> {
        let lines: Vec<&str> = text.lines().collect();
        let mut names = Vec::new();
        let mut in_section = false;
        for (idx, raw) in lines.iter().enumerate() {
            let line = raw.trim();
            if line == section_kw {
                in_section = true;
                continue;
            }
            if !in_section {
                continue;
            }
            // Section ends at a top-level closing bracket (unindented).
            if !raw.starts_with(char::is_whitespace) && (line == "}" || line == ")" || line == ");")
            {
                break;
            }
            if line.ends_with('{') && idx > 0 {
                let prev = lines[idx - 1].trim();
                if prev == section_kw {
                    // This is the section's opening brace, not a patch.
                    continue;
                }
                if !prev.is_empty() && prev.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    names.push(prev.to_string());
                }
            }
        }
        names
    }

    #[tokio::test]
    async fn generated_case_is_a_consistent_baseline() {
        // The generated case must be a complete, runnable baseline: every
        // dictionary brace-balanced, and the patch set agreeing across
        // blockMeshDict, U, and p so OpenFOAM will not reject unknown patches.
        let dir = std::env::temp_dir().join(format!("oc-test-baseline-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();

        create_case(&dir, false).await.unwrap();

        let read = |rel: &str| std::fs::read_to_string(dir.join(rel)).unwrap_or_default();

        for rel in [
            "system/blockMeshDict",
            "system/snappyHexMeshDict",
            "system/controlDict",
            "system/fvSchemes",
            "system/fvSolution",
            "system/decomposeDict",
            "constant/transportProperties",
            "0/U",
            "0/p",
        ] {
            let s = read(rel);
            let open = s.chars().filter(|c| *c == '{').count();
            let close = s.chars().filter(|c| *c == '}').count();
            assert_eq!(
                open, close,
                "unbalanced braces in {rel}: {open} open vs {close} close"
            );
        }

        let mut mesh = patch_names(&read("system/blockMeshDict"), "boundary");
        let mut u = patch_names(&read("0/U"), "boundaryField");
        let mut p = patch_names(&read("0/p"), "boundaryField");
        mesh.sort();
        u.sort();
        p.sort();
        assert!(!mesh.is_empty(), "expected at least one patch");
        assert_eq!(mesh, u, "blockMeshDict and U patch sets differ");
        assert_eq!(mesh, p, "blockMeshDict and p patch sets differ");

        // The generated templates must use valid OpenFOAM syntax, not just be
        // internally consistent: field files use `boundaryField` (not the
        // misspelled `boundaryCondition`), the internal field has no stray
        // `value` keyword, and blockMesh faces are nested vertex lists.
        for field in ["0/U", "0/p"] {
            let text = read(field);
            assert!(
                text.contains("boundaryField"),
                "{field} must use the OpenFOAM `boundaryField` keyword"
            );
            assert!(
                !text.contains("boundaryCondition"),
                "{field} must not use the invalid `boundaryCondition` spelling"
            );
            assert!(
                !text.contains("internalField   value") && !text.contains("internalField value"),
                "{field} internalField must be `internalField uniform ...` without a stray `value`"
            );
        }
        let mesh_text = read("system/blockMeshDict");
        assert!(
            mesh_text.contains("((0 4 7 3))"),
            "blockMeshDict faces must be nested vertex lists, e.g. ((0 4 7 3))"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
