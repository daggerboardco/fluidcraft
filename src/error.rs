//! Error types for Fluidcraft.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum FluidcraftError {
    #[error("OpenFOAM not found on PATH; install OpenFOAM (or distro-clean) and retry")]
    OpenfoamMissing,

    #[error("OpenFOAM command `{command}` failed (exit {code}): {message}")]
    OpenfoamFailed {
        command: String,
        code: i32,
        message: String,
    },

    #[error("OpenFOAM output could not be parsed: {0}")]
    Parse(String),

    #[error("geometry import failed: {0}")]
    Geometry(String),

    #[error("meshing failed: {0}")]
    Meshing(String),

    #[error("io error for {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}
