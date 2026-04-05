use std::io::Write;
use std::path::Path;

const CLAUDE_SOURCE_IMAGE: &str = "ghcr.io/ablack94/docker-claude:stable";

const DOCKERFILE_TEMPLATE: &str = include_str!("templates/Dockerfile");
const ENTRYPOINT_SCRIPT: &str = include_str!("templates/entrypoint.sh");
const SQUID_CONF_TEMPLATE: &str = include_str!("templates/squid.conf");
const SIMPLE_COMPOSE_TEMPLATE: &str = include_str!("templates/compose-simple.yaml");
const ISOLATED_COMPOSE_TEMPLATE: &str = include_str!("templates/compose-isolated.yaml");

/// Generate a squid.conf that only allows the given hostnames.
fn generate_squid_conf(allowed_hosts: &[&str]) -> String {
    let domain_acls: String = allowed_hosts
        .iter()
        .map(|host| format!("acl allowed_domains dstdomain {host}"))
        .collect::<Vec<_>>()
        .join("\n");

    SQUID_CONF_TEMPLATE.replace("{{DOMAIN_ACLS}}", &domain_acls)
}

/// Generate common substitution values.
fn format_env_file_volumes_command(
    profile: Option<&str>,
    mounts: &[(String, String)],
    args: &[String],
) -> (String, String, String) {
    let auth = match crate::auth::resolve_auth(profile) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Warning: {e}");
            crate::auth::ResolvedAuth { profile_env: None, host_api_key: None }
        }
    };

    if !auth.has_auth() {
        eprintln!("Warning: No authentication configured. Set ANTHROPIC_API_KEY or create a profile with `claude-container auth create <name>`.");
    }

    let mut env_files = Vec::new();

    if let Some(path) = &auth.profile_env {
        env_files.push(format!("      - {}", path.display()));
    }

    // Write host API key to a separate env file so it can be referenced alongside the profile
    if let Some(key) = &auth.host_api_key {
        if let Ok(path) = crate::auth::write_host_api_key_env(key) {
            env_files.push(format!("      - {}", path.display()));
        }
    }

    let env_file = if env_files.is_empty() {
        String::new()
    } else {
        format!("    env_file:\n{}\n", env_files.join("\n"))
    };

    let volumes = if mounts.is_empty() {
        String::new()
    } else {
        let entries: String = mounts
            .iter()
            .map(|(host, container)| format!("      - {host}:{container}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("    volumes:\n{entries}\n")
    };

    let command = if args.is_empty() {
        String::new()
    } else {
        let entries: String = args
            .iter()
            .map(|a| format!("      - \"{a}\""))
            .collect::<Vec<_>>()
            .join("\n");
        format!("    command:\n{entries}\n")
    };

    (env_file, volumes, command)
}

/// Collapse runs of blank lines into a single blank line and trim trailing whitespace.
fn clean_yaml(s: String) -> String {
    let mut out = Vec::new();
    let mut prev_blank = false;
    for line in s.lines() {
        let blank = line.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        out.push(line);
        prev_blank = blank;
    }
    // Trim trailing blank lines, ensure single trailing newline
    while out.last().map_or(false, |l| l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n") + "\n"
}

/// Generate a simple compose.yaml (no network isolation).
fn generate_simple_compose(
    profile: Option<&str>,
    mounts: &[(String, String)],
    args: &[String],
    uid: u32,
    gid: u32,
) -> String {
    let (env_file, volumes, command) =
        format_env_file_volumes_command(profile, mounts, args);

    clean_yaml(
        SIMPLE_COMPOSE_TEMPLATE
            .replace("{{UID}}", &uid.to_string())
            .replace("{{GID}}", &gid.to_string())
            .replace("{{ENV_FILE}}", &env_file)
            .replace("{{VOLUMES}}", &volumes)
            .replace("{{COMMAND}}", &command),
    )
}

/// Generate a compose.yaml for network-isolated mode.
fn generate_isolated_compose(
    profile: Option<&str>,
    squid_conf_path: &str,
    mounts: &[(String, String)],
    args: &[String],
    uid: u32,
    gid: u32,
) -> String {
    let (env_file, volumes, command) =
        format_env_file_volumes_command(profile, mounts, args);

    clean_yaml(
        ISOLATED_COMPOSE_TEMPLATE
            .replace("{{UID}}", &uid.to_string())
            .replace("{{GID}}", &gid.to_string())
            .replace("{{SQUID_CONF_PATH}}", squid_conf_path)
            .replace("{{ENV_FILE}}", &env_file)
            .replace("{{VOLUMES}}", &volumes)
            .replace("{{COMMAND}}", &command),
    )
}

/// Write the Dockerfile and entrypoint script into the project directory.
fn write_dockerfile(dir: &Path, base_image: &str) -> Result<(), String> {
    let content = DOCKERFILE_TEMPLATE
        .replace("{{BASE_IMAGE}}", base_image)
        .replace("{{CLAUDE_SOURCE_IMAGE}}", CLAUDE_SOURCE_IMAGE);

    let dockerfile_path = dir.join("Dockerfile");
    let mut f = std::fs::File::create(&dockerfile_path)
        .map_err(|e| format!("Failed to write Dockerfile: {e}"))?;
    f.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write Dockerfile: {e}"))?;

    let entrypoint_path = dir.join("entrypoint.sh");
    let mut f = std::fs::File::create(&entrypoint_path)
        .map_err(|e| format!("Failed to write entrypoint.sh: {e}"))?;
    f.write_all(ENTRYPOINT_SCRIPT.as_bytes())
        .map_err(|e| format!("Failed to write entrypoint.sh: {e}"))?;

    // Make entrypoint executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&entrypoint_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to set entrypoint.sh permissions: {e}"))?;
    }

    Ok(())
}

/// Write out a simple (non-isolated) compose project.
pub fn write_simple_project(
    dir: &Path,
    base_image: &str,
    profile: Option<&str>,
    mounts: &[(String, String)],
    args: &[String],
    uid: u32,
    gid: u32,
) -> Result<std::path::PathBuf, String> {
    write_dockerfile(dir, base_image)?;

    let compose_path = dir.join("compose.yaml");
    let content = generate_simple_compose(profile, mounts, args, uid, gid);
    let mut f = std::fs::File::create(&compose_path)
        .map_err(|e| format!("Failed to write compose.yaml: {e}"))?;
    f.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write compose.yaml: {e}"))?;
    Ok(compose_path)
}

/// Write out a network-isolated compose project (with squid gateway).
pub fn write_isolated_project(
    dir: &Path,
    base_image: &str,
    profile: Option<&str>,
    extra_hosts: &[String],
    mounts: &[(String, String)],
    args: &[String],
    uid: u32,
    gid: u32,
) -> Result<std::path::PathBuf, String> {
    write_dockerfile(dir, base_image)?;

    let mut hosts: Vec<&str> = vec![".anthropic.com", ".claude.com"];
    for h in extra_hosts {
        if !hosts.contains(&h.as_str()) {
            hosts.push(h.as_str());
        }
    }

    // Write squid.conf
    let squid_conf_path = dir.join("squid.conf");
    {
        let mut f = std::fs::File::create(&squid_conf_path)
            .map_err(|e| format!("Failed to write squid.conf: {e}"))?;
        f.write_all(generate_squid_conf(&hosts).as_bytes())
            .map_err(|e| format!("Failed to write squid.conf: {e}"))?;
    }

    // Write compose.yaml
    let compose_path = dir.join("compose.yaml");
    {
        let mut f = std::fs::File::create(&compose_path)
            .map_err(|e| format!("Failed to write compose.yaml: {e}"))?;
        f.write_all(
            generate_isolated_compose(profile, "./squid.conf", mounts, args, uid, gid).as_bytes(),
        )
        .map_err(|e| format!("Failed to write compose.yaml: {e}"))?;
    }

    eprintln!("Allowed hosts: {}", hosts.join(", "));
    Ok(compose_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_squid_conf_contains_required_hosts() {
        let conf = generate_squid_conf(&[".anthropic.com", ".claude.com"]);
        assert!(conf.contains("acl allowed_domains dstdomain .anthropic.com"));
        assert!(conf.contains("acl allowed_domains dstdomain .claude.com"));
        assert!(conf.contains("http_access deny all"));
    }

    #[test]
    fn test_isolated_compose_structure() {
        let yml = generate_isolated_compose(
            None,
            "/tmp/squid.conf",
            &[("/home/user/.claude".into(), "/home/claude/.claude".into())],
            &[],
            1000,
            1000,
        );
        assert!(yml.contains("gateway:"));
        assert!(yml.contains("claude:"));
        assert!(yml.contains("build: ."));
        assert!(yml.contains("internal: true"));
        assert!(yml.contains("HTTPS_PROXY=http://gateway"));
        assert!(yml.contains("user: \"1000:1000\""));
    }

    #[test]
    fn test_simple_compose_structure() {
        let yml = generate_simple_compose(
            None,
            &[("/work".into(), "/workarea".into())],
            &[],
            1000,
            1000,
        );
        assert!(yml.contains("claude:"));
        assert!(yml.contains("network_mode: host"));
        assert!(yml.contains("user: \"1000:1000\""));
        assert!(!yml.contains("gateway:"));
        assert!(!yml.contains("internal: true"));
    }
}
