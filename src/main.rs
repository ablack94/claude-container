mod build;
mod run;
mod runtime;

use clap::{Parser, Subcommand};
use runtime::Runtime;

#[derive(Parser)]
#[command(name = "claude-container", about = "Build and run Claude in any container image")]
struct Cli {
    /// Container runtime to use (default: auto-detect)
    #[arg(long, global = true)]
    runtime: Option<Runtime>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build (if needed) and launch Claude in a container
    Run {
        /// Base image to use (e.g. ubuntu:24.04)
        base_image: String,

        /// Force rebuild, ignoring cache
        #[arg(long)]
        rebuild: bool,

        /// Additional arguments passed to Claude inside the container
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Build the Claude container image without running it
    Build {
        /// Base image to use (e.g. ubuntu:24.04)
        base_image: String,

        /// Force rebuild, ignoring cache
        #[arg(long)]
        rebuild: bool,

        /// Custom output tag for the built image
        #[arg(long)]
        tag: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    let rt = match cli.runtime {
        Some(r) => r,
        None => Runtime::detect().unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }),
    };

    let result = match cli.command {
        Commands::Build {
            base_image,
            rebuild,
            tag,
        } => build::ensure_image(rt, &base_image, tag.as_deref(), rebuild).map(|_| ()),

        Commands::Run {
            base_image,
            rebuild,
            args,
        } => build::ensure_image(rt, &base_image, None, rebuild)
            .and_then(|tag| run::launch(rt, &tag, &args)),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
