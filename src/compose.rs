use std::io::Write;
use std::path::Path;

const CLAUDE_IMAGE_BASE: &str = "ghcr.io/ablack94/docker-claude";
const DEFAULT_VERSION: &str = "stable";

/// Build the full claude source image reference for a given version tag.
pub fn claude_source_image(version: Option<&str>) -> String {
    let tag = version.unwrap_or(DEFAULT_VERSION);
    format!("{CLAUDE_IMAGE_BASE}:{tag}")
}

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

/// Render extra tmpfs mounts, one per container path, owned by the host user.
///
/// Paths nested inside the /home/claude tmpfs are otherwise created by the
/// runtime as root-owned mountpoints, which the container (running as the host
/// user) cannot write to.
fn format_tmpfs_extra(paths: &[String], uid: u32, gid: u32) -> String {
    if paths.is_empty() {
        return String::new();
    }
    paths
        .iter()
        .map(|p| format!("      - {p}:uid={uid},gid={gid}\n"))
        .collect()
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
            // The profile is unusable, but host credentials are independent of
            // it and are still forwarded.
            crate::auth::ResolvedAuth::host_only()
        }
    };

    if auth.has_auth() {
        eprintln!("Auth: {}", auth.describe());
    } else {
        eprintln!(
            "Warning: No authentication configured. Export {oauth} or {key} on the host, \
             or create a profile with `claude-container auth create <name> oauth <token>` \
             (get a long-lived token from `claude setup-token`).",
            oauth = crate::auth::OAUTH_TOKEN_VAR,
            key = crate::auth::API_KEY_VAR,
        );
    }

    let mut env_files = Vec::new();

    if let Some(path) = &auth.profile_env {
        env_files.push(format!("      - {}", path.display()));
    }

    // Host credentials go in a separate env file so they can be referenced
    // alongside the profile. The profile is listed first, but only variables it
    // leaves unset reach this file, so neither clobbers the other.
    if !auth.host_env.is_empty() {
        match crate::auth::write_host_env(&auth.host_env) {
            Ok(path) => env_files.push(format!("      - {}", path.display())),
            Err(e) => eprintln!("Warning: {e}"),
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
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n") + "\n"
}

/// Generate a simple compose.yaml (no network isolation).
fn generate_simple_compose(
    profile: Option<&str>,
    mounts: &[(String, String)],
    extra_tmpfs: &[String],
    args: &[String],
    uid: u32,
    gid: u32,
) -> String {
    let (env_file, volumes, command) = format_env_file_volumes_command(profile, mounts, args);

    clean_yaml(
        SIMPLE_COMPOSE_TEMPLATE
            .replace("{{UID}}", &uid.to_string())
            .replace("{{GID}}", &gid.to_string())
            .replace("{{TMPFS_EXTRA}}", &format_tmpfs_extra(extra_tmpfs, uid, gid))
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
    extra_tmpfs: &[String],
    args: &[String],
    uid: u32,
    gid: u32,
) -> String {
    let (env_file, volumes, command) = format_env_file_volumes_command(profile, mounts, args);

    clean_yaml(
        ISOLATED_COMPOSE_TEMPLATE
            .replace("{{UID}}", &uid.to_string())
            .replace("{{GID}}", &gid.to_string())
            .replace("{{TMPFS_EXTRA}}", &format_tmpfs_extra(extra_tmpfs, uid, gid))
            .replace("{{SQUID_CONF_PATH}}", squid_conf_path)
            .replace("{{ENV_FILE}}", &env_file)
            .replace("{{VOLUMES}}", &volumes)
            .replace("{{COMMAND}}", &command),
    )
}

/// Render the Dockerfile for a base image and the UID/GID the container runs as.
///
/// The UID/GID are baked in so the image can guarantee an `/etc/passwd` entry
/// for the runtime user; see the template for why that matters.
fn generate_dockerfile(base_image: &str, version: Option<&str>, uid: u32, gid: u32) -> String {
    let source_image = claude_source_image(version);
    DOCKERFILE_TEMPLATE
        .replace("{{BASE_IMAGE}}", base_image)
        .replace("{{CLAUDE_SOURCE_IMAGE}}", &source_image)
        .replace("{{UID}}", &uid.to_string())
        .replace("{{GID}}", &gid.to_string())
}

/// Write the Dockerfile and entrypoint script into the project directory.
fn write_dockerfile(
    dir: &Path,
    base_image: &str,
    version: Option<&str>,
    uid: u32,
    gid: u32,
) -> Result<(), String> {
    let content = generate_dockerfile(base_image, version, uid, gid);

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
#[allow(clippy::too_many_arguments)]
pub fn write_simple_project(
    dir: &Path,
    base_image: &str,
    profile: Option<&str>,
    mounts: &[(String, String)],
    extra_tmpfs: &[String],
    args: &[String],
    uid: u32,
    gid: u32,
    version: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    write_dockerfile(dir, base_image, version, uid, gid)?;

    let compose_path = dir.join("compose.yaml");
    let content = generate_simple_compose(profile, mounts, extra_tmpfs, args, uid, gid);
    let mut f = std::fs::File::create(&compose_path)
        .map_err(|e| format!("Failed to write compose.yaml: {e}"))?;
    f.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write compose.yaml: {e}"))?;
    Ok(compose_path)
}

/// Write out a network-isolated compose project (with squid gateway).
#[allow(clippy::too_many_arguments)]
pub fn write_isolated_project(
    dir: &Path,
    base_image: &str,
    profile: Option<&str>,
    extra_hosts: &[String],
    mounts: &[(String, String)],
    extra_tmpfs: &[String],
    args: &[String],
    uid: u32,
    gid: u32,
    version: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    write_dockerfile(dir, base_image, version, uid, gid)?;

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

    // Bind it by absolute path, like every other mount we emit: a relative one
    // resolves against whatever directory the compose file is invoked from.
    let squid_conf_abs = std::fs::canonicalize(&squid_conf_path)
        .map_err(|e| format!("Failed to resolve {}: {e}", squid_conf_path.display()))?;
    let squid_conf_ref = squid_conf_abs
        .to_str()
        .ok_or_else(|| format!("Path is not valid UTF-8: {}", squid_conf_abs.display()))?;

    // Write compose.yaml
    let compose_path = dir.join("compose.yaml");
    {
        let mut f = std::fs::File::create(&compose_path)
            .map_err(|e| format!("Failed to write compose.yaml: {e}"))?;
        f.write_all(
            generate_isolated_compose(profile, squid_conf_ref, mounts, extra_tmpfs, args, uid, gid)
                .as_bytes(),
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
    fn test_dockerfile_substitutes_every_placeholder() {
        let df = generate_dockerfile("ubuntu:24.04", Some("nightly"), 1234, 5678);
        assert!(df.starts_with("FROM ubuntu:24.04\n"));
        assert!(df.contains("--from=ghcr.io/ablack94/docker-claude:nightly"));
        assert!(!df.contains("{{"), "unsubstituted placeholder in:\n{df}");
    }

    #[test]
    fn test_dockerfile_adds_a_passwd_entry_for_the_runtime_uid() {
        // The Dev Containers extension runs `id -un`, which fails outright when
        // the runtime UID is missing from /etc/passwd.
        let df = generate_dockerfile("ubuntu:24.04", None, 1234, 5678);
        assert!(df.contains("getent passwd 1234"));
        assert!(df.contains("echo \"claude:x:1234:5678::/home/claude:/bin/sh\" >> /etc/passwd"));
        assert!(df.contains("getent group 5678"));
        assert!(df.contains("echo \"claude:x:5678:\" >> /etc/group"));
        // `getent` is missing from some minimal images; a grep fallback covers it.
        assert!(df.contains("command -v getent"));
        assert!(df.contains("grep -q \"^[^:]*:[^:]*:1234:\" /etc/passwd"));
        assert!(df.contains("grep -q \"^[^:]*:[^:]*:5678:\" /etc/group"));
        // The entries must exist before anything runs as that user.
        let passwd = df.find("/etc/passwd").expect("passwd line");
        assert!(passwd < df.find("ENTRYPOINT").expect("entrypoint line"));
    }

    #[test]
    fn test_workspace_is_mounted_relative_to_the_compose_file() {
        // `..` resolves against .claude-container/, i.e. the project directory,
        // so the mount survives a clone or a moved checkout.
        let workspace = [("..".to_string(), "/workarea".to_string())];
        for yml in [
            generate_simple_compose(None, &workspace, &[], &[], 1000, 1000),
            generate_isolated_compose(None, "/abs/squid.conf", &workspace, &[], &[], 1000, 1000),
        ] {
            assert!(yml.contains("      - ..:/workarea\n"), "in:\n{yml}");
        }
    }

    #[test]
    fn test_isolated_compose_structure() {
        let yml = generate_isolated_compose(
            None,
            "/tmp/squid.conf",
            &[("/home/user/.claude".into(), "/home/claude/.claude".into())],
            &[],
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
    fn test_isolated_project_binds_squid_conf_by_absolute_path() {
        let dir = std::env::temp_dir().join(format!("cc-squid-path-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let compose_path = write_isolated_project(
            &dir,
            "ubuntu:24.04",
            None,
            &[],
            &[("..".into(), "/workarea".into())],
            &[],
            &[],
            1000,
            1000,
            None,
        )
        .unwrap();

        let yml = std::fs::read_to_string(&compose_path).unwrap();
        // Relative bind sources resolve against the invoking directory, not the
        // compose file's; every path we emit but the workspace is absolute.
        assert!(!yml.contains("./squid.conf"), "in:\n{yml}");
        let line = yml
            .lines()
            .find(|l| l.contains("/etc/squid/squid.conf:ro"))
            .expect("squid bind mount");
        let host_path = line.trim().trim_start_matches("- ");
        assert!(host_path.starts_with('/'), "not absolute: {line}");
        assert!(host_path.ends_with("squid.conf:/etc/squid/squid.conf:ro"));
        assert!(std::path::Path::new(host_path.split(':').next().unwrap()).is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_extra_tmpfs_is_owned_by_the_host_user() {
        let yml = generate_simple_compose(
            None,
            &[("/work".into(), "/workarea".into())],
            &["/home/claude/.claude".to_string()],
            &[],
            1000,
            1000,
        );
        assert!(yml.contains("- /home/claude:uid=1000,gid=1000"));
        assert!(yml.contains("- /home/claude/.claude:uid=1000,gid=1000"));
    }

    #[test]
    fn test_simple_compose_structure() {
        let yml = generate_simple_compose(
            None,
            &[("/work".into(), "/workarea".into())],
            &[],
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
