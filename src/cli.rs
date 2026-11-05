//! Command-line interface for Fluidcraft.

use crate::error::FluidcraftError;
use crate::geometry;
use crate::of;
use anyhow::Result;
use clap::{Parser, Subcommand};

/// A CFD application with an OpenFOAM backend.
#[derive(Parser, Debug)]
#[command(name = "fluidcraft", version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Locate and validate the OpenFOAM installation.
    Check,

    /// Generate an OpenFOAM case directory for a given meshing profile.
    New {
        /// Output path for the new OpenFOAM case.
        path: std::path::PathBuf,
        /// Overwrite an existing case directory (refused by default).
        #[arg(long)]
        overwrite: bool,
    },

    /// Run the meshing pipeline (blockMesh + snappyHexMesh).
    Mesh {
        /// Path to the OpenFOAM case directory.
        path: std::path::PathBuf,
    },

    /// Run a solver over an OpenFOAM case directory.
    Solve {
        /// Path to the OpenFOAM case directory.
        path: std::path::PathBuf,
        /// Solver to run (e.g. simpleFoam, kSSTFoam).
        #[arg(long, default_value = "simpleFoam")]
        solver: String,
    },

    /// Import a STEP model into a triangulated surface (STL).
    Geometry {
        #[command(subcommand)]
        command: GeometryCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum GeometryCommand {
    /// Convert a STEP file to a normalized ASCII STL via solidcraft.
    Convert {
        /// Input STEP file.
        input: std::path::PathBuf,
        /// Output STL file (normalized ASCII).
        #[arg(long, short)]
        out: std::path::PathBuf,
        /// Unit scale applied to vertices (1.0 = as-is, 0.001 = mm->m).
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Path to the solidcraft-cli binary (else $FLUIDCRAFT_SOLIDCRAFT / PATH).
        #[arg(long)]
        solidcraft: Option<std::path::PathBuf>,
    },
}

pub async fn run(args: Args) -> Result<()> {
    match args.command {
        Command::Check => check().await,
        Command::New { path, overwrite } => new(path, overwrite).await,
        Command::Mesh { path } => mesh(path).await,
        Command::Solve { path, solver } => solve(path, solver).await,
        Command::Geometry { command } => geometry(command).await,
    }
}

/// Verify OpenFOAM is available and report the version.
async fn check() -> Result<()> {
    let version = of::openfoam_version().map_err(|e| {
        let msg = e.to_string();
        let missing = matches!(
            e.downcast::<FluidcraftError>(),
            Ok(FluidcraftError::OpenfoamMissing)
        );
        anyhow::anyhow!(
            "{}",
            if missing {
                "OpenFOAM not found. Install it, then re-run `fluidcraft check`.".to_string()
            } else {
                msg
            }
        )
    })?;
    println!("OpenFOAM found: {version}");
    Ok(())
}

async fn new(path: std::path::PathBuf, overwrite: bool) -> Result<()> {
    of::create_case(&path, overwrite).await?;
    println!("Created OpenFOAM case at {}", path.display());
    Ok(())
}

async fn mesh(path: std::path::PathBuf) -> Result<()> {
    of::mesh(&path).await?;
    println!("Meshing complete for {}", path.display());
    Ok(())
}

async fn solve(path: std::path::PathBuf, solver: String) -> Result<()> {
    of::solve(&path, &solver).await?;
    println!("Solve complete for {}", path.display());
    Ok(())
}

/// Run a geometry subcommand.
async fn geometry(command: GeometryCommand) -> Result<()> {
    match command {
        GeometryCommand::Convert {
            input,
            out,
            scale,
            solidcraft,
        } => {
            let bin = geometry::locate_solidcraft(solidcraft.as_deref())?;
            geometry::step_to_stl(&input, &out, &bin, scale)?;
            let mesh = geometry::read_stl(&out)?;
            println!(
                "Converted {} -> {} ({} triangles, scale {})",
                input.display(),
                out.display(),
                mesh.triangles.len(),
                scale
            );
            Ok(())
        }
    }
}
