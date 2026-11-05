//! Meshing dictionary generation for OpenFOAM cases.
//!
//! Generates `blockMeshDict` and `snappyHexMeshDict` from a geometry bounding
//! box and an imported STL surface. The defaults reproduce the snappyHexMesh
//! configuration proven to mesh a solidcraft-exported washer STL end-to-end in
//! the Fluidcraft integration spike (OpenFOAM 14).
//!
//! These are pure string generators — no OpenFOAM binary required — so they are
//! unit-testable without a solver installed.

/// Axis-aligned bounding box of a triangulated surface, in solver units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl BoundingBox {
    /// Center point of the box.
    pub fn center(&self) -> [f64; 3] {
        [
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
            (self.min[2] + self.max[2]) / 2.0,
        ]
    }

    /// Largest edge length of the box.
    pub fn max_extent(&self) -> f64 {
        (self.max[0] - self.min[0])
            .max(self.max[1] - self.min[1])
            .max(self.max[2] - self.min[2])
    }
}

/// Parameters controlling mesh generation.
#[derive(Debug, Clone)]
pub struct MeshingConfig {
    /// Bounding box of the imported geometry (must fit inside the blockMesh box).
    pub geometry: BoundingBox,
    /// Extra margin added around the geometry when sizing the blockMesh box.
    pub box_margin: f64,
    /// Refinement level applied to cells intersecting the surface.
    pub surface_level: [u32; 2],
    /// Refinement level for cells inside the solid region.
    pub inside_level: u32,
    /// Global cell cap.
    pub max_global_cells: usize,
    /// Local cell cap before load balancing.
    pub max_local_cells: usize,
    /// Whether to add prism (boundary-layer) inflation layers.
    pub add_layers: bool,
    /// Final boundary-layer thickness as a fraction of the smallest edge.
    pub layer_final_thickness: f64,
    /// Boundary-layer count.
    pub layer_count: usize,
    /// Boundary-layer expansion ratio.
    pub layer_expansion_ratio: f64,
}

impl Default for MeshingConfig {
    fn default() -> Self {
        Self {
            geometry: BoundingBox {
                min: [0.0, 0.0, 0.0],
                max: [1.0, 1.0, 1.0],
            },
            box_margin: 0.0,
            surface_level: [1, 2],
            inside_level: 0,
            max_global_cells: 500_000,
            max_local_cells: 100_000,
            add_layers: false,
            layer_final_thickness: 0.5,
            layer_count: 5,
            layer_expansion_ratio: 1.2,
        }
    }
}

/// Generate a `blockMeshDict` hex box that encloses `bbox` with `margin` added
/// on every side. The box has the six named boundary patches (`inlet`,
/// `outlet`, `lowerWall`, `upperWall`, `sideWalls`) used by the Fluidcraft
/// baseline fields, so generated cases stay consistent with `U`/`p`.
pub fn generate_block_mesh_dict(bbox: &BoundingBox, margin: f64) -> String {
    let min = [
        bbox.min[0] - margin,
        bbox.min[1] - margin,
        bbox.min[2] - margin,
    ];
    let max = [
        bbox.max[0] + margin,
        bbox.max[1] + margin,
        bbox.max[2] + margin,
    ];

    // 8 corner vertices in the standard OpenFOAM hex ordering, so the boundary
    // patch faces below line up with the baseline BCs:
    //   0: min,min,min   1: max,min,min   2: max,max,min   3: min,max,min
    //   4: min,min,max   5: max,min,max   6: max,max,max   7: min,max,max
    let corners: [[f64; 3]; 8] = [
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], max[1], min[2]],
        [min[0], max[1], min[2]],
        [min[0], min[1], max[2]],
        [max[0], min[1], max[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ];
    let fmt = |p: [f64; 3]| format!("{:.9e} {:.9e} {:.9e}", p[0], p[1], p[2]);

    let mut s = String::new();
    s.push_str(&foam_file_header("blockMeshDict"));
    s.push('\n');

    s.push_str("vertices\n(\n");
    for c in corners {
        s.push_str(&format!("    ({})\n", fmt(c)));
    }
    s.push_str(");\n\n");

    s.push_str("blocks\n(\n");
    s.push_str("    hex (0 1 2 3 4 5 6 7) (1 1 1) simpleGrading (1 1 1)\n");
    s.push_str(");\n\n");

    s.push_str("edges\n(\n);\n\n");

    s.push_str("boundary\n(\n");
    // inlet = x-min face (0 4 7 3); outlet = x-max face (1 2 6 5).
    s.push_str("    inlet\n    {\n        type patch;\n        faces ((0 4 7 3));\n    }\n");
    s.push_str("    outlet\n    {\n        type patch;\n        faces ((1 2 6 5));\n    }\n");
    s.push_str("    lowerWall\n    {\n        type patch;\n        faces ((0 1 5 4));\n    }\n");
    s.push_str("    upperWall\n    {\n        type patch;\n        faces ((3 7 6 2));\n    }\n");
    s.push_str("    sideWalls\n    {\n        type patch;\n        faces\n        (\n            ( 0 3 2 1 )\n            ( 4 5 6 7 )\n        );\n    }\n");
    s.push_str(");\n\n");

    s.push_str("mergePatchPairs\n(\n);\n");
    s
}

/// Generate a `snappyHexMeshDict` referencing the given STL surface.
///
/// `surface_name` and `surface_file` must match the entry written into the case
/// `constant/geometry/` directory. The generated dict includes every keyword
/// the integration spike proved necessary (inside/outside points, refinement
/// surfaces/regions, and a complete `meshQualityControls` block).
pub fn generate_snappy_hex_mesh_dict(config: &MeshingConfig, surface_name: &str, surface_file: &str) -> String {
    let center = config.geometry.center();
    let margin = config.box_margin;
    // A point clearly outside the blockMesh box, on the +x/+y/+z corner.
    let outside = [
        config.geometry.max[0] + margin + 1.0,
        config.geometry.max[1] + margin + 1.0,
        config.geometry.max[2] + margin + 1.0,
    ];

    let mut s = String::new();
    s.push_str(&foam_file_header("snappyHexMeshDict"));
    s.push('\n');

    s.push_str("castellatedMesh on;\n");
    s.push_str("snap            on;\n");
    s.push_str(&format!("addLayers       {};\n\n", bool_word(config.add_layers)));

    // Geometry section: the imported triSurface plus an inside refinement region.
    s.push_str("geometry\n{\n");
    s.push_str(&format!(
        "    {surface_name}\n    {{\n        type triSurface;\n        file \"{surface_file}\";\n    }}\n\n"
    ));
    if config.inside_level > 0 {
        s.push_str(&format!(
            "    {surface_name}_inside\n    {{\n        type surfaces;\n        surfaces (\"{surface_file}\");\n        mode inside;\n        level {level};\n    }}\n\n",
            level = config.inside_level
        ));
    }
    s.push_str("};\n\n");

    // Castellated mesh controls.
    s.push_str("castellatedMeshControls\n{\n");
    s.push_str(&format!("    maxLocalCells {};\n", config.max_local_cells));
    s.push_str(&format!("    maxGlobalCells {};\n", config.max_global_cells));
    s.push_str("    minRefinementCells 10;\n");
    s.push_str("    nCellsBetweenLevels 3;\n");
    s.push_str(&format!(
        "    insidePoint ({:.9e} {:.9e} {:.9e});\n",
        center[0], center[1], center[2]
    ));
    s.push_str(&format!(
        "    outsidePoint ({:.9e} {:.9e} {:.9e});\n",
        outside[0], outside[1], outside[2]
    ));
    s.push_str("    allowFreeStandingZoneFaces true;\n");
    s.push_str("    resolveFeatureAngle 30;\n\n");

    s.push_str("    refinementSurfaces\n    {\n");
    s.push_str(&format!(
        "        {surface_name}\n        {{\n            level ({l} {h});\n            patchInfo\n            {{\n                type wall;\n            }}\n        }}\n    }}\n\n",
        l = config.surface_level[0],
        h = config.surface_level[1]
    ));

    s.push_str("    refinementRegions\n    {\n");
    if config.inside_level > 0 {
        s.push_str(&format!(
            "        {surface_name}_inside\n        {{\n            mode inside;\n            level {level};\n        }}\n\n",
            level = config.inside_level
        ));
    }
    s.push_str("    };\n};\n\n");

    // Snapping controls.
    s.push_str("snapControls\n{\n");
    s.push_str("    nSmoothPatch 3;\n");
    s.push_str("    tolerance 2.0;\n");
    s.push_str("    nSolveIter 30;\n");
    s.push_str("    nRelaxIter 5;\n");
    s.push_str("    nFeatureSnapIter 15;\n");
    s.push_str("    implicitFeatureSnap false;\n");
    s.push_str("    explicitFeatureSnap true;\n");
    s.push_str("    multiRegionFeatureSnap false;\n");
    s.push_str("};\n\n");

    // Layer controls.
    s.push_str("addLayersControls\n{\n");
    s.push_str("    relativeSizes true;\n");
    // When boundary layers are enabled, emit a `layers` subdict with a
    // per-patch `nSurfaceLayers` for every wall (audit 2026-11-05). snappyHexMesh
    // calls subDict("layers") unconditionally, so this must exist or it throws.
    if config.add_layers {
        s.push_str("    layers\n    {\n");
        for patch in [
            "lowerWall",
            "upperWall",
            "sideWalls",
            surface_name,
        ] {
            s.push_str(&format!(
                "        {patch}\n        {{\n            nSurfaceLayers {count};\n        }}\n",
                count = config.layer_count
            ));
        }
        s.push_str("    }\n\n");
    }
    s.push_str(&format!("    expansionRatio {:.6};\n", config.layer_expansion_ratio));
    s.push_str(&format!("    finalLayerThickness {:.6};\n", config.layer_final_thickness));
    s.push_str("    minThickness 1e-3;\n");
    s.push_str("    nGrow 0;\n");
    s.push_str("    featureAngle 100;\n");
    s.push_str("    nRelaxIter 3;\n");
    s.push_str("    nSmoothNormals 3;\n");
    s.push_str("    nSmoothThickness 2;\n");
    s.push_str("    maxFaceThicknessRatio 0.5;\n");
    s.push_str("    maxThicknessToMedialRatio 0.3;\n");
    s.push_str("    minMedianAxisAngle 90;\n");
    s.push_str("    nBufferCellsNoExtrude 0;\n");
    s.push_str("    nLayerIter 50;\n");
    s.push_str("};\n\n");

    // Mesh quality controls (complete set proven in the spike).
    s.push_str("meshQualityControls\n{\n");
    s.push_str("    maxNonOrtho 65;\n");
    s.push_str("    maxFaceSkew 20;\n");
    s.push_str("    maxExtensionAngle 40;\n");
    s.push_str("    minQuality 0.3;\n");
    s.push_str("    minVol 1e-13;\n");
    s.push_str("    minTetQuality 1e-14;\n");
    s.push_str("    maxConcave 120;\n");
    s.push_str("    maxInternalSkewness 4;\n");
    s.push_str("    maxBoundarySkewness 20;\n");
    s.push_str("    minFaceWeight 0.1;\n");
    s.push_str("    minVolRatio 0.1;\n");
    s.push_str("    minTwist 0.05;\n");
    s.push_str("    minDeterminant 0.05;\n");
    s.push_str("    errorReduction 0.5;\n");
    s.push_str("    nSmoothScale 3;\n");
    s.push_str("};\n\n");

    s.push_str("mergeTolerance 1e-6;\n");
    s
}

fn bool_word(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

fn foam_file_header(object: &str) -> String {
    format!(
        "/* Fluidcraft {object} */\nFoamFile {{\n    version     2.0;\n    format      ascii;\n    class       dictionary;\n    location    \"system\";\n    object      {object};\n}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn washer_bbox() -> BoundingBox {
        BoundingBox {
            min: [-0.02, -0.02, 0.0],
            max: [0.02, 0.02, 0.003],
        }
    }

    #[test]
    fn block_mesh_dict_has_all_five_patches() {
        let dict = generate_block_mesh_dict(&washer_bbox(), 0.01);
        for patch in ["inlet", "outlet", "lowerWall", "upperWall", "sideWalls"] {
            assert!(dict.contains(patch), "missing patch {patch}");
        }
        // Brace balance.
        assert_eq!(
            dict.matches('{').count(),
            dict.matches('}').count(),
            "unbalanced braces"
        );
    }

    #[test]
    fn block_mesh_box_encloses_geometry() {
        let dict = generate_block_mesh_dict(&washer_bbox(), 0.01);
        // Box should extend past the geometry min/max by the margin (0.01),
        // rendered in 9-decimal scientific notation.
        assert!(dict.contains("-3.000000000e-2"), "box should extend below -0.02");
        assert!(dict.contains("3.000000000e-2"), "box should extend above 0.02");
    }

    #[test]
    fn snappy_dict_references_surface_and_has_required_keys() {
        let config = MeshingConfig {
            geometry: washer_bbox(),
            box_margin: 0.01,
            ..Default::default()
        };
        let dict = generate_snappy_hex_mesh_dict(&config, "washer", "washer.stl");

        assert!(dict.contains("type triSurface;"));
        assert!(dict.contains("\"washer.stl\""));
        assert!(dict.contains("refinementSurfaces"));
        assert!(dict.contains("patchInfo"));
        assert!(dict.contains("insidePoint"));
        assert!(dict.contains("outsidePoint"));
        assert!(dict.contains("allowFreeStandingZoneFaces"));
        // Full meshQualityControls proven set.
        for key in [
            "minVol", "minTetQuality", "maxConcave", "maxInternalSkewness",
            "maxBoundarySkewness", "minFaceWeight", "minVolRatio", "minTwist",
            "minDeterminant", "errorReduction", "nSmoothScale",
        ] {
            assert!(dict.contains(key), "missing meshQualityControls key {key}");
        }
        assert_eq!(
            dict.matches('{').count(),
            dict.matches('}').count(),
            "unbalanced braces"
        );
    }

    #[test]
    fn snappy_dict_adds_inside_region_when_level_positive() {
        let mut config = MeshingConfig {
            geometry: washer_bbox(),
            box_margin: 0.01,
            inside_level: 2,
            ..Default::default()
        };
        config.add_layers = true;
        let dict = generate_snappy_hex_mesh_dict(&config, "washer", "washer.stl");
        assert!(dict.contains("on;"), "addLayers on expected");
        assert!(dict.contains("washer_inside"));
        assert!(dict.contains("mode inside;"));
        // A `layers` subdict with per-patch nSurfaceLayers must exist when
        // addLayers is on, else snappyHexMesh's subDict("layers") throws.
        assert!(dict.contains("layers"), "layers subdict expected");
        assert!(dict.contains("nSurfaceLayers"), "nSurfaceLayers expected");
        assert_eq!(
            dict.matches('{').count(),
            dict.matches('}').count(),
            "unbalanced braces"
        );
    }

    #[test]
    fn snappy_dict_emits_no_layers_subdict_when_disabled() {
        let config = MeshingConfig {
            geometry: washer_bbox(),
            box_margin: 0.01,
            ..Default::default()
        };
        assert!(!config.add_layers);
        let dict = generate_snappy_hex_mesh_dict(&config, "washer", "washer.stl");
        assert!(dict.contains("addLayers       off;"));
        // No per-patch layer definitions when layers are off.
        assert!(!dict.contains("nSurfaceLayers"));
        assert_eq!(
            dict.matches('{').count(),
            dict.matches('}').count(),
            "unbalanced braces"
        );
    }
}
