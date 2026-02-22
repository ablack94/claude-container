use std::fmt;
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub enum Runtime {
    Docker,
    Podman,
}

impl Runtime {
    /// Auto-detect container runtime by checking PATH.
    pub fn detect() -> Result<Self, String> {
        if command_exists("docker") {
            Ok(Runtime::Docker)
        } else if command_exists("podman") {
            Ok(Runtime::Podman)
        } else {
            Err("No container runtime found. Install docker or podman and ensure it is on PATH.".into())
        }
    }

    /// Return the CLI command name.
    pub fn cmd(&self) -> &'static str {
        match self {
            Runtime::Docker => "docker",
            Runtime::Podman => "podman",
        }
    }
}

impl fmt::Display for Runtime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.cmd())
    }
}

impl std::str::FromStr for Runtime {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "docker" => Ok(Runtime::Docker),
            "podman" => Ok(Runtime::Podman),
            other => Err(format!("Unknown runtime '{}'. Use 'docker' or 'podman'.", other)),
        }
    }
}

fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
