use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

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

/// Environment variable carrying a long-lived OAuth token (from
/// `claude setup-token`). This is what the Claude Agent SDK reads.
pub const OAUTH_TOKEN_VAR: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// Environment variable carrying a raw API key.
pub const API_KEY_VAR: &str = "ANTHROPIC_API_KEY";

/// Credential variables forwarded from the host environment. Both are
/// forwarded when the host sets both; there is no precedence between them.
const CREDENTIAL_VARS: &[&str] = &[OAUTH_TOKEN_VAR, API_KEY_VAR];

/// Name of the env file holding credentials scraped from the host environment.
const HOST_ENV_FILE: &str = "host.env";

/// Resolved authentication: a profile env file plus any host credentials that
/// the profile does not already define.
pub struct ResolvedAuth {
    /// Name of the active profile, if one was resolved.
    pub profile_name: Option<String>,
    /// Path to the profile's .env file (e.g. ~/.config/claude-container/profiles/work.env)
    pub profile_env: Option<PathBuf>,
    /// Credential variables taken from the host environment, as (name, value).
    pub host_env: Vec<(String, String)>,
}

impl ResolvedAuth {
    pub fn has_auth(&self) -> bool {
        self.profile_env.is_some() || !self.host_env.is_empty()
    }

    /// A resolution with no profile but with whatever credentials the host
    /// environment provides. Used when profile resolution fails, so host
    /// credentials are still forwarded rather than silently dropped.
    pub fn host_only() -> Self {
        ResolvedAuth {
            profile_name: None,
            profile_env: None,
            host_env: select_host_env(&[], |var| std::env::var(var).ok()),
        }
    }

    /// Human-readable summary of where credentials came from.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(name) = &self.profile_name {
            parts.push(format!("profile '{name}'"));
        }
        for (var, _) in &self.host_env {
            parts.push(format!("host {var}"));
        }
        if parts.is_empty() {
            "none".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Read the variable names defined by an env file. Blank lines and `#`
/// comments are skipped; anything else is treated as `NAME=value`.
fn env_file_vars(path: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            line.split_once('=')
                .map(|(name, _)| name.trim().to_string())
        })
        .collect()
}

/// Pick the credential variables to forward from the host.
///
/// The active profile is authoritative for the variables it defines, so a
/// host value only fills in a variable the profile leaves unset. `lookup`
/// reads the host environment.
fn select_host_env<F>(defined: &[String], lookup: F) -> Vec<(String, String)>
where
    F: Fn(&str) -> Option<String>,
{
    CREDENTIAL_VARS
        .iter()
        .filter(|var| !defined.iter().any(|d| d == *var))
        .filter_map(|var| {
            lookup(var)
                .filter(|value| !value.is_empty())
                .map(|value| ((*var).to_string(), value))
        })
        .collect()
}

/// Resolve authentication for a build.
pub fn resolve_auth(profile: Option<&str>) -> Result<ResolvedAuth, String> {
    let profile_name = match profile {
        Some(name) => Some(name.to_string()),
        None => default_profile(),
    };

    let profile_env = match &profile_name {
        Some(name) => {
            let path = profile_path(name)?;
            if !path.exists() {
                return Err(format!("Profile '{name}' does not exist. Create it with:\n  claude-container auth create {name} oauth <token>"));
            }
            Some(path)
        }
        None => None,
    };

    let defined = profile_env
        .as_deref()
        .map(env_file_vars)
        .unwrap_or_default();
    let host_env = select_host_env(&defined, |var| std::env::var(var).ok());

    Ok(ResolvedAuth {
        profile_name,
        profile_env,
        host_env,
    })
}

/// Write host credentials to an env file (0600) for compose to reference.
pub fn write_host_env(vars: &[(String, String)]) -> Result<PathBuf, String> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;

    let path = dir.join(HOST_ENV_FILE);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("Failed to write host env file: {e}"))?;

    let content: String = vars
        .iter()
        .map(|(name, value)| format!("{name}={value}\n"))
        .collect();
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write host env file: {e}"))?;

    // `.mode()` above only applies when the file is created, so tighten the
    // permissions explicitly in case the file already existed.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("Failed to set host env file permissions: {e}"))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn host<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn test_host_oauth_token_is_forwarded() {
        let selected = select_host_env(&[], host(&[(OAUTH_TOKEN_VAR, "sk-ant-oat01-abc")]));
        assert_eq!(
            selected,
            vec![(OAUTH_TOKEN_VAR.to_string(), "sk-ant-oat01-abc".to_string())]
        );
    }

    #[test]
    fn test_host_api_key_is_forwarded() {
        let selected = select_host_env(&[], host(&[(API_KEY_VAR, "sk-ant-api03-xyz")]));
        assert_eq!(
            selected,
            vec![(API_KEY_VAR.to_string(), "sk-ant-api03-xyz".to_string())]
        );
    }

    #[test]
    fn test_profile_wins_over_host_for_the_same_var() {
        // An oauth profile defines the token, so the host value is dropped...
        let defined = vec![OAUTH_TOKEN_VAR.to_string()];
        let selected = select_host_env(
            &defined,
            host(&[
                (OAUTH_TOKEN_VAR, "sk-ant-oat01-host"),
                (API_KEY_VAR, "sk-ant-api03-host"),
            ]),
        );
        // ...but the API key it leaves unset still comes through.
        assert_eq!(
            selected,
            vec![(API_KEY_VAR.to_string(), "sk-ant-api03-host".to_string())]
        );
    }

    #[test]
    fn test_fallback_forwards_every_host_credential() {
        // `ResolvedAuth::host_only()` (the profile-resolution failure path)
        // selects with no profile-defined vars, so every credential the host
        // sets is forwarded.
        let selected = select_host_env(
            &[],
            host(&[
                (OAUTH_TOKEN_VAR, "sk-ant-oat01-host"),
                (API_KEY_VAR, "sk-ant-api03-host"),
            ]),
        );
        assert_eq!(
            selected,
            vec![
                (OAUTH_TOKEN_VAR.to_string(), "sk-ant-oat01-host".to_string()),
                (API_KEY_VAR.to_string(), "sk-ant-api03-host".to_string()),
            ]
        );

        // With nothing in the host environment the fallback carries no auth.
        let empty = ResolvedAuth {
            profile_name: None,
            profile_env: None,
            host_env: select_host_env(&[], host(&[])),
        };
        assert!(!empty.has_auth());
    }

    #[test]
    fn test_empty_host_values_are_ignored() {
        assert!(select_host_env(&[], host(&[(OAUTH_TOKEN_VAR, "")])).is_empty());
        assert!(select_host_env(&[], host(&[])).is_empty());
    }

    #[test]
    fn test_unrelated_host_vars_are_not_forwarded() {
        assert!(select_host_env(&[], host(&[("PATH", "/usr/bin")])).is_empty());
    }

    #[test]
    fn test_env_file_vars_reads_profile_keys() {
        let dir = std::env::temp_dir().join(format!("cc-auth-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("profile.env");
        std::fs::write(
            &path,
            "# a comment\n\nCLAUDE_CODE_OAUTH_TOKEN=sk-ant-oat01-abc\n",
        )
        .unwrap();

        assert_eq!(env_file_vars(&path), vec![OAUTH_TOKEN_VAR.to_string()]);
        assert!(env_file_vars(&dir.join("missing.env")).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_describe_names_every_source() {
        let auth = ResolvedAuth {
            profile_name: Some("work".to_string()),
            profile_env: Some(PathBuf::from("/tmp/work.env")),
            host_env: vec![(API_KEY_VAR.to_string(), "k".to_string())],
        };
        assert!(auth.has_auth());
        assert_eq!(auth.describe(), "profile 'work', host ANTHROPIC_API_KEY");

        let empty = ResolvedAuth {
            profile_name: None,
            profile_env: None,
            host_env: Vec::new(),
        };
        assert!(!empty.has_auth());
        assert_eq!(empty.describe(), "none");
    }
}
