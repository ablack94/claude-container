use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

/// Return the path to ~/.config/claude-container/.
pub fn config_dir() -> Result<PathBuf, String> {
    let home =
        std::env::var("HOME").map_err(|_| "HOME environment variable not set".to_string())?;
    Ok(PathBuf::from(home).join(".config").join("claude-container"))
}

/// Return the path to ~/.config/claude-container/profiles/.
fn profiles_dir() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("profiles"))
}

/// Return the path to a named profile's env file.
fn profile_path(name: &str) -> Result<PathBuf, String> {
    Ok(profiles_dir()?.join(format!("{name}.env")))
}

/// Return the path to the default-profile marker file.
fn default_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("default"))
}

/// Read the default profile name, if set.
pub fn default_profile() -> Option<String> {
    let path = default_path().ok()?;
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Set the default profile name.
pub fn set_default_profile(name: &str) -> Result<(), String> {
    // Verify the profile exists
    let path = profile_path(name)?;
    if !path.exists() {
        return Err(format!("Profile '{name}' does not exist"));
    }

    let default = default_path()?;
    std::fs::write(&default, name).map_err(|e| format!("Failed to write default profile: {e}"))?;
    eprintln!("Default profile set to '{name}'");
    Ok(())
}

/// List all profile names.
pub fn list_profiles() -> Result<Vec<String>, String> {
    let dir = profiles_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read profiles directory: {e}"))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            name.strip_suffix(".env").map(|n| n.to_string())
        })
        .collect();

    names.sort();
    Ok(names)
}

/// Remove a profile by name.
pub fn remove_profile(name: &str) -> Result<(), String> {
    let path = profile_path(name)?;
    if !path.exists() {
        return Err(format!("Profile '{name}' does not exist"));
    }

    std::fs::remove_file(&path).map_err(|e| format!("Failed to remove profile: {e}"))?;

    // Clear default if it pointed to this profile
    if default_profile().as_deref() == Some(name) {
        let _ = std::fs::remove_file(default_path()?);
    }

    eprintln!("Removed profile '{name}'");
    Ok(())
}

/// Write a profile env file with the given content (0600 permissions).
fn write_profile_env(name: &str, content: &str) -> Result<PathBuf, String> {
    let dir = profiles_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;

    let path = profile_path(name)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("Failed to write profile: {e}"))?;

    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write profile: {e}"))?;

    Ok(path)
}

/// Resolved authentication: a profile env file path and/or a host API key.
pub struct ResolvedAuth {
    /// Path to the profile's .env file (e.g. ~/.config/claude-container/profiles/work.env)
    pub profile_env: Option<PathBuf>,
    /// ANTHROPIC_API_KEY from the host environment, if set
    pub host_api_key: Option<String>,
}

impl ResolvedAuth {
    pub fn has_auth(&self) -> bool {
        self.profile_env.is_some() || self.host_api_key.is_some()
    }
}

/// Resolve authentication for a build.
/// Returns the profile env file path (if any) and host API key (if any).
pub fn resolve_auth(profile: Option<&str>) -> Result<ResolvedAuth, String> {
    let host_api_key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());

    let profile_name = match profile {
        Some(name) => Some(name.to_string()),
        None => default_profile(),
    };

    let profile_env = match &profile_name {
        Some(name) => {
            let path = profile_path(name)?;
            if !path.exists() {
                return Err(format!("Profile '{name}' does not exist. Create it with:\n  claude-container auth create {name}"));
            }
            Some(path)
        }
        None => None,
    };

    Ok(ResolvedAuth {
        profile_env,
        host_api_key,
    })
}

/// Write the host ANTHROPIC_API_KEY to a small env file for compose to reference.
pub fn write_host_api_key_env(key: &str) -> Result<PathBuf, String> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;

    let path = dir.join("host-api-key.env");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("Failed to write host API key env file: {e}"))?;

    file.write_all(format!("ANTHROPIC_API_KEY={key}").as_bytes())
        .map_err(|e| format!("Failed to write host API key env file: {e}"))?;

    Ok(path)
}

/// Create an OAuth token profile.
pub fn create_oauth_profile(name: &str, token: &str) -> Result<(), String> {
    if token.is_empty() {
        return Err("OAuth token cannot be empty".to_string());
    }
    let path = write_profile_env(name, &format!("CLAUDE_CODE_OAUTH_TOKEN={token}"))?;
    eprintln!("Profile '{name}' saved to {}", path.display());
    Ok(())
}

/// Create an API key profile.
pub fn create_api_key_profile(name: &str, key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    let path = write_profile_env(name, &format!("ANTHROPIC_API_KEY={key}"))?;
    eprintln!("Profile '{name}' saved to {}", path.display());
    Ok(())
}
