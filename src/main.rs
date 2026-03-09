mod auth;
mod compose;
mod run;
mod runtime;

use clap::{Parser, Subcommand};
use runtime::Runtime;

#[derive(Parser)]
#[command(name = "claude-container", about = "Build and run Claude in any container image")]
struct Cli {
    /// Container runtime to use (overrides configured default)
    #[arg(long, global = true)]
    runtime: Option<Runtime>,

    /// Change to this directory before running (like `git -C`)
    #[arg(short = 'C', global = true)]
    directory: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build the .claude-container/ project from a base image
    Build {
        /// Base image to use (e.g. ubuntu:24.04)
        base_image: String,

        /// Auth profile to use (default: the profile set via `auth default`)
        #[arg(long)]
        profile: Option<String>,

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

        /// Immediately run after building
        #[arg(long)]
        run: bool,
    },

    /// Run the existing .claude-container/ project
    Run {
        /// Force rebuild of the container image
        #[arg(long)]
        rebuild: bool,
    },

    /// Manage authentication profiles
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },

    /// Configure runtime settings
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Remove the .claude-container/ directory from the current project
    Clean,
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Create a new auth profile (runs `claude setup-token` for OAuth)
    Create {
        /// Profile name
        name: String,

        /// Store an API key instead of an OAuth token
        #[arg(long)]
        api_key: bool,

        /// Set this profile as the default
        #[arg(long)]
        default: bool,
    },

    /// List all auth profiles
    List,

    /// Set or show the default auth profile
    Default {
        /// Profile name to set as default (omit to show current)
        name: Option<String>,
    },

    /// Remove an auth profile
    Remove {
        /// Profile name to remove
        name: String,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Set or show the default container runtime
    Runtime {
        /// Runtime to set as default (omit to show current)
        name: Option<Runtime>,

        /// Clear the default runtime (use auto-detect)
        #[arg(long)]
        clear: bool,
    },

    /// Ban or unban a container runtime
    Ban {
        /// Runtime to ban
        name: Runtime,

        /// Remove the ban instead of adding it
        #[arg(long)]
        remove: bool,
    },

    /// Show current configuration
    Show,
}

fn handle_config(command: ConfigCommands) -> Result<(), String> {
    match command {
        ConfigCommands::Runtime { name, clear } => {
            if clear {
                let mut config = runtime::RuntimeConfig::load();
                config.clear_default();
                config.save()?;
                eprintln!("Default runtime cleared (will auto-detect).");
            } else if let Some(rt) = name {
                let mut config = runtime::RuntimeConfig::load();
                if config.banned.contains(&rt) {
                    return Err(format!("Cannot set '{rt}' as default — it is banned. Remove the ban first with:\n  claude-container config ban {rt} --remove"));
                }
                config.set_default(rt);
                config.save()?;
                eprintln!("Default runtime set to '{rt}'.");
            } else {
                let config = runtime::RuntimeConfig::load();
                match config.default {
                    Some(rt) => println!("{rt}"),
                    None => eprintln!("No default runtime set (auto-detect)."),
                }
            }
        }
        ConfigCommands::Ban { name, remove } => {
            let mut config = runtime::RuntimeConfig::load();
            if remove {
                config.remove_ban(name);
            } else {
                config.add_ban(name);
            }
            config.save()?;
            if remove {
                eprintln!("Removed ban on '{name}'.");
            } else {
                eprintln!("Banned runtime '{name}'.");
            }
        }
        ConfigCommands::Show => {
            let config = runtime::RuntimeConfig::load();
            match config.default {
                Some(rt) => println!("runtime: {rt}"),
                None => println!("runtime: auto-detect"),
            }
            if config.banned.is_empty() {
                println!("banned: (none)");
            } else {
                let names: Vec<_> = config.banned.iter().map(|r| r.to_string()).collect();
                println!("banned: {}", names.join(", "));
            }
        }
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();

    // Handle -C directory change
    if let Some(ref dir) = cli.directory {
        if let Err(e) = std::env::set_current_dir(dir) {
            eprintln!("Error: Failed to change directory to '{dir}': {e}");
            std::process::exit(1);
        }
    }

    // Auth and Config don't need a container runtime
    match cli.command {
        Commands::Auth { command } => {
            let result = match command {
                AuthCommands::Create { name, api_key, default } => {
                    let result = if api_key {
                        auth::create_api_key_profile(&name)
                    } else {
                        auth::create_oauth_profile(&name)
                    };
                    result.and_then(|()| {
                        if default {
                            auth::set_default_profile(&name)
                        } else {
                            Ok(())
                        }
                    })
                }
                AuthCommands::List => {
                    match auth::list_profiles() {
                        Ok(profiles) => {
                            if profiles.is_empty() {
                                eprintln!("No auth profiles configured.");
                                eprintln!("Create one with: claude-container auth create <name>");
                            } else {
                                let default = auth::default_profile();
                                for name in &profiles {
                                    if default.as_deref() == Some(name.as_str()) {
                                        println!("* {name} (default)");
                                    } else {
                                        println!("  {name}");
                                    }
                                }
                            }
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                AuthCommands::Default { name } => {
                    match name {
                        Some(name) => auth::set_default_profile(&name),
                        None => {
                            match auth::default_profile() {
                                Some(name) => {
                                    println!("{name}");
                                    Ok(())
                                }
                                None => {
                                    eprintln!("No default profile set.");
                                    eprintln!("Set one with: claude-container auth default <name>");
                                    Ok(())
                                }
                            }
                        }
                    }
                }
                AuthCommands::Remove { name } => auth::remove_profile(&name),
            };

            if let Err(e) = result {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
            return;
        }

        Commands::Config { command } => {
            let result = handle_config(command);

            if let Err(e) = result {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
            return;
        }

        Commands::Clean => {
            let dir = std::path::Path::new(".claude-container");
            if dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(dir) {
                    eprintln!("Error: Failed to remove .claude-container/: {e}");
                    std::process::exit(1);
                }
                eprintln!("Removed .claude-container/");
            } else {
                eprintln!("Nothing to clean — .claude-container/ does not exist.");
            }
            return;
        }

        _ => {}
    }

    let rt = Runtime::resolve(cli.runtime).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let result = match cli.command {
        Commands::Auth { .. } | Commands::Config { .. } | Commands::Clean => unreachable!(),

        Commands::Build {
            base_image,
            profile,
            isolated,
            allow_hosts,
            forward_settings,
            args,
            run: should_run,
        } => {
            let use_isolation = isolated || !allow_hosts.is_empty();
            run::build(
                &base_image,
                profile.as_deref(),
                use_isolation,
                &allow_hosts,
                forward_settings,
                &args,
            ).and_then(|_| {
                if should_run {
                    run::run(rt, false)
                } else {
                    Ok(())
                }
            })
        }

        Commands::Run { rebuild } => run::run(rt, rebuild),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
