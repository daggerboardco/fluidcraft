//! Fluidcraft — a CFD application with an OpenFOAM backend.
//!
//! Linux-native, parallel, and GPU-capable. Mesh creation is the core focus;
//! the solver backend is OpenFOAM, invoked as a subprocess library.
//!
//! See `roadmap.md` for the full project plan.

pub mod cli;
pub mod error;
pub mod geometry;
pub mod of;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Args::parse();
    cli::run(args).await
}
