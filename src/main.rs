mod auth;
mod build;
mod compose;
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

        /// Run with network isolation (only whitelisted hosts reachable)
        #[arg(long)]
        isolated: bool,

        /// Additional hostnames to allow through the proxy (implies --isolated).
        /// api.anthropic.com is always allowed.
        #[arg(long = "allow-host", num_args = 1)]
        allow_hosts: Vec<String>,

        /// Mount host ~/.claude and ~/.claude.json into the container
        #[arg(long)]
        forward_settings: bool,

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

    /// Authenticate and store a long-lived API token
    Auth,

    /// Manage docker-compose project for network-isolated mode
    Compose {
        #[command(subcommand)]
        action: ComposeAction,
    },
}

#[derive(Subcommand)]
enum ComposeAction {
    /// Render the compose project files to a local directory
    Render {
        /// Base image to use (e.g. ubuntu:24.04)
        base_image: String,

        /// Additional hostnames to allow through the proxy.
        /// api.anthropic.com is always allowed.
        #[arg(long = "allow-host", num_args = 1)]
        allow_hosts: Vec<String>,

        /// User to run the claude container as (e.g. 1000:1000)
        #[arg(long)]
        user: Option<String>,

        /// Mount host ~/.claude and ~/.claude.json into the container
        #[arg(long)]
        forward_settings: bool,

        /// Output directory for compose files (default: ./.claude-container)
        #[arg(long, short)]
        output: Option<String>,

        /// Additional arguments passed to Claude inside the container
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Run a previously rendered compose project
    Run {
        /// Path to compose project directory (default: ./.claude-container)
        #[arg(long, short)]
        dir: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    // Auth doesn't need a container runtime
    if matches!(cli.command, Commands::Auth) {
        if let Err(e) = auth::setup_token() {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
        return;
    }

    let rt = match cli.runtime {
        Some(r) => r,
        None => Runtime::detect().unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }),
    };

    let result = match cli.command {
        Commands::Auth => unreachable!(),
        Commands::Build {
            base_image,
            rebuild,
            tag,
        } => build::ensure_image(rt, &base_image, tag.as_deref(), rebuild).map(|_| ()),

        Commands::Run {
            base_image,
            rebuild,
            isolated,
            allow_hosts,
            forward_settings,
            args,
        } => {
            let use_isolation = isolated || !allow_hosts.is_empty();
            build::ensure_image(rt, &base_image, None, rebuild)
                .and_then(|tag| {
                    if use_isolation {
                        run::launch_isolated(rt, &tag, &allow_hosts, forward_settings, &args)
                    } else {
                        run::launch(rt, &tag, forward_settings, &args)
                    }
                })
        }

        Commands::Compose { action } => match action {
            ComposeAction::Render {
                base_image,
                allow_hosts,
                user,
                forward_settings,
                output,
                args,
            } => {
                run::render_compose(&base_image, &allow_hosts, user.as_deref(), forward_settings, output.as_deref(), &args)
            }
            ComposeAction::Run { dir } => {
                run::run_compose(rt, dir.as_deref())
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
