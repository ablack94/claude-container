use std::process::Command;

use crate::runtime::Runtime;

/// Launch the container with appropriate volume mounts.
pub fn launch(runtime: Runtime, tag: &str, args: &[String]) -> Result<(), String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {e}"))?;

    let claude_dir = format!("{home}/.claude");
    let claude_json = format!("{home}/.claude.json");
    let workdir = cwd.to_str().ok_or("Current directory path is not valid UTF-8")?;

    let mut cmd = Command::new(runtime.cmd());
    cmd.args(["run", "--rm", "-it"]);

    // Mount ~/.claude if it exists
    if std::path::Path::new(&claude_dir).exists() {
        cmd.args(["-v", &format!("{claude_dir}:/root/.claude")]);
    }

    // Mount ~/.claude.json if it exists
    if std::path::Path::new(&claude_json).exists() {
        cmd.args(["-v", &format!("{claude_json}:/root/.claude.json")]);
    }

    // Mount current directory as /workarea
    cmd.args(["-v", &format!("{workdir}:/workarea")]);
    cmd.args(["-w", "/workarea"]);

    // Image
    cmd.arg(tag);

    // Pass-through args (these go to the entrypoint/CMD)
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
