use std::process::Command;

use crate::compose;
use crate::runtime::Runtime;

/// Determine the container home directory based on the --user flag.
/// If a username is given, use /home/<username>. If a UID or no user, use /root.
fn container_home(user: Option<&str>) -> String {
    match user {
        None => "/root".to_string(),
        Some(u) => {
            // Extract the user part before any :group
            let username = u.split(':').next().unwrap_or(u);
            // If it looks like a UID (all digits), check if it's 0 (root)
            if username.chars().all(|c| c.is_ascii_digit()) {
                if username == "0" {
                    "/root".to_string()
                } else {
                    format!("/home/{username}")
                }
            } else if username == "root" {
                "/root".to_string()
            } else {
                format!("/home/{username}")
            }
        }
    }
}

/// Collect the standard volume mounts as (host, container) pairs.
fn collect_mounts(user: Option<&str>, forward_settings: bool) -> Result<Vec<(String, String)>, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {e}"))?;
    let workdir = cwd.to_str().ok_or("Current directory path is not valid UTF-8")?.to_string();
    let chome = container_home(user);

    let mut mounts = Vec::new();

    if forward_settings {
        let claude_dir = format!("{home}/.claude");
        if std::path::Path::new(&claude_dir).exists() {
            mounts.push((claude_dir, format!("{chome}/.claude")));
        }

        let claude_json = format!("{home}/.claude.json");
        if std::path::Path::new(&claude_json).exists() {
            mounts.push((claude_json, format!("{chome}/.claude.json")));
        }
    }

    mounts.push((workdir, "/workarea".to_string()));
    Ok(mounts)
}

/// Launch the container with appropriate volume mounts.
pub fn launch(runtime: Runtime, tag: &str, forward_settings: bool, args: &[String]) -> Result<(), String> {
    let mounts = collect_mounts(None, forward_settings)?;

    let mut cmd = Command::new(runtime.cmd());
    cmd.args(["run", "--rm", "-it"]);

    for (host, container) in &mounts {
        cmd.args(["-v", &format!("{host}:{container}")]);
    }
    cmd.args(["-w", "/workarea"]);

    if let Some(token) = crate::auth::load_token() {
        cmd.args(["-e", &format!("CLAUDE_API_KEY={token}")]);
    }

    cmd.arg(tag);

    if !args.is_empty() {
        cmd.args(args);
    }

    let status = cmd
        .status()
        .map_err(|e| format!("Failed to run {} run: {e}", runtime.cmd()))?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

/// Render the compose project files to a local directory for inspection/customization.
pub fn render_compose(
    base_image: &str,
    allow_hosts: &[String],
    user: Option<&str>,
    forward_settings: bool,
    output: Option<&str>,
    args: &[String],
) -> Result<(), String> {
    let mounts = collect_mounts(user, forward_settings)?;
    let tag = crate::build::sanitize_tag(base_image);

    let out_dir = std::path::Path::new(output.unwrap_or(".claude-container"));
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("Failed to create output directory: {e}"))?;

    let compose_path = compose::write_compose_project(
        out_dir,
        &tag,
        allow_hosts,
        &mounts,
        user,
        args,
    )?;

    eprintln!("Compose project written to {}", out_dir.display());
    eprintln!("To run:");
    eprintln!("  claude-container compose run");
    eprintln!("  # or: docker compose -f {} run --rm claude", compose_path.display());

    Ok(())
}

/// Run a previously rendered compose project.
pub fn run_compose(runtime: Runtime, dir: Option<&str>) -> Result<(), String> {
    let compose_dir = std::path::Path::new(dir.unwrap_or(".claude-container"));
    let compose_file = compose_dir.join("compose.yaml");

    if !compose_file.exists() {
        return Err(format!(
            "No compose.yaml found in {}. Run 'claude-container compose render' first.",
            compose_dir.display()
        ));
    }

    let compose_file_str = compose_file.to_str().ok_or("Non-UTF-8 path")?;

    let status = Command::new(runtime.cmd())
        .args([
            "compose",
            "-f", compose_file_str,
            "run", "--rm",
            "--service-ports",
            "claude",
        ])
        .status()
        .map_err(|e| format!("Failed to run {} compose: {e}", runtime.cmd()))?;

    // Always tear down on exit
    let _ = Command::new(runtime.cmd())
        .args(["compose", "-f", compose_file_str, "down"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

/// Launch in network-isolated mode using docker compose with a squid proxy gateway.
pub fn launch_isolated(
    runtime: Runtime,
    tag: &str,
    allow_hosts: &[String],
    forward_settings: bool,
    args: &[String],
) -> Result<(), String> {
    let mounts = collect_mounts(None, forward_settings)?;

    let compose_dir = std::path::Path::new(".claude-container");
    std::fs::create_dir_all(compose_dir)
        .map_err(|e| format!("Failed to create .claude-container directory: {e}"))?;

    let compose_path = compose::write_compose_project(
        compose_dir,
        tag,
        allow_hosts,
        &mounts,
        None,
        args,
    )?;

    eprintln!("Starting network-isolated session...");

    let compose_file = compose_path.to_str().ok_or("Non-UTF-8 path")?;

    // Use `docker compose` (v2) or `podman compose`
    let status = Command::new(runtime.cmd())
        .args([
            "compose",
            "-f", compose_file,
            "run", "--rm",
            "--service-ports",
            "claude",
        ])
        .status()
        .map_err(|e| format!("Failed to run {} compose: {e}", runtime.cmd()))?;

    // Always tear down on exit
    let _ = Command::new(runtime.cmd())
        .args(["compose", "-f", compose_file, "down"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
