use std::fmt;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Docker,
    Podman,
}

impl Runtime {
    /// Resolve the runtime to use: CLI flag > config default > auto-detect.
    /// Banned runtimes are always rejected.
    pub fn resolve(cli_override: Option<Runtime>) -> Result<Self, String> {
        let config = RuntimeConfig::load();
        let rt = match cli_override {
            Some(r) => r,
            None => match config.default {
                Some(r) => {
                    if !command_exists(r.cmd()) {
                        return Err(format!(
                            "Default runtime '{}' is not installed or not on PATH.",
                            r
                        ));
                    }
                    r
                }
                None => Self::detect(&config.banned)?,
            },
        };

        if config.banned.contains(&rt) {
            return Err(format!(
                "Runtime '{}' is banned. Change this with:\n  claude-container config runtime-ban --remove {}",
                rt, rt
            ));
        }

        Ok(rt)
    }

    /// Auto-detect container runtime by checking PATH, skipping banned ones.
    fn detect(banned: &[Runtime]) -> Result<Self, String> {
        for candidate in &[Runtime::Docker, Runtime::Podman] {
            if !banned.contains(candidate) && command_exists(candidate.cmd()) {
                return Ok(*candidate);
            }
        }
        Err("No container runtime found. Install docker or podman and ensure it is on PATH.".into())
    }

    /// Return the CLI command name.
    pub fn cmd(&self) -> &'static str {
        match self {
            Runtime::Docker => "docker",
            Runtime::Podman => "podman",
        }
    }

    /// Return the command and initial args needed to run compose.
    /// Checks `<runtime> compose` first, then falls back to `docker-compose`
    /// or `podman-compose` standalone tools.
    pub fn compose_cmd(&self) -> Result<(String, Vec<String>), String> {
        // Try `<runtime> compose` (builtin plugin)
        let builtin_works = Command::new(self.cmd())
            .args(["compose", "version"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if builtin_works {
            return Ok((self.cmd().to_string(), vec!["compose".to_string()]));
        }

        // Try standalone `<runtime>-compose`
        let standalone = format!("{}-compose", self.cmd());
        if command_exists(&standalone) {
            return Ok((standalone, vec![]));
        }

        Err(format!(
            "No compose support found for '{}'.\n\
             Install either the compose plugin (`{} compose`) or the standalone `{}-compose` tool.",
            self.cmd(), self.cmd(), self.cmd()
        ))
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

/// Runtime configuration stored in ~/.config/claude-container/config.
pub struct RuntimeConfig {
    pub default: Option<Runtime>,
    pub banned: Vec<Runtime>,
}

impl RuntimeConfig {
    fn config_path() -> Result<std::path::PathBuf, String> {
        crate::auth::config_dir().map(|d| d.join("config"))
    }

    /// Load config from disk. Returns defaults if file doesn't exist.
    pub fn load() -> Self {
        let path = match Self::config_path() {
            Ok(p) => p,
            Err(_) => return Self { default: None, banned: Vec::new() },
        };

        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self { default: None, banned: Vec::new() },
        };

        let mut default = None;
        let mut banned = Vec::new();

        for line in contents.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("default=") {
                default = value.parse::<Runtime>().ok();
            } else if let Some(value) = line.strip_prefix("ban=") {
                if let Ok(rt) = value.parse::<Runtime>() {
                    if !banned.contains(&rt) {
                        banned.push(rt);
                    }
                }
            }
        }

        Self { default, banned }
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {e}"))?;
        }

        let mut lines = Vec::new();
        if let Some(rt) = self.default {
            lines.push(format!("default={rt}"));
        }
        for rt in &self.banned {
            lines.push(format!("ban={rt}"));
        }

        std::fs::write(&path, lines.join("\n") + "\n")
            .map_err(|e| format!("Failed to write config: {e}"))?;
        Ok(())
    }

    pub fn set_default(&mut self, rt: Runtime) {
        self.default = Some(rt);
    }

    pub fn clear_default(&mut self) {
        self.default = None;
    }

    pub fn add_ban(&mut self, rt: Runtime) {
        if !self.banned.contains(&rt) {
            self.banned.push(rt);
        }
        // If banning the default, clear it
        if self.default == Some(rt) {
            self.default = None;
        }
    }

    pub fn remove_ban(&mut self, rt: Runtime) {
        self.banned.retain(|&r| r != rt);
    }
}
