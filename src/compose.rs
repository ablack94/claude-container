use std::io::Write;
use std::path::Path;

const SQUID_CONF_TEMPLATE: &str = r#"http_port 3128
pid_filename /tmp/squid.pid

# Allowed destinations
{{DOMAIN_ACLS}}

# HTTPS (CONNECT) and HTTP access control
acl SSL_ports port 443
acl CONNECT method CONNECT

http_access allow CONNECT SSL_ports allowed_domains
http_access allow allowed_domains
http_access deny all

# Logging
access_log stdio:/dev/stderr
cache_log stdio:/dev/stderr

# No caching
cache deny all
"#;

const COMPOSE_TEMPLATE: &str = r#"services:
  gateway:
    image: ubuntu/squid:latest
    user: proxy
    volumes:
      - {{SQUID_CONF_PATH}}:/etc/squid/squid.conf:ro
    networks:
      - external
      - internal
    healthcheck:
      test: ["CMD-SHELL", "bash -c 'echo > /dev/tcp/127.0.0.1/3128'"]
      interval: 3s
      timeout: 5s
      retries: 10

  claude:
    image: {{IMAGE_TAG}}
    init: true
    stdin_open: true
    tty: true
    environment:
      - HTTP_PROXY=http://gateway:3128
      - HTTPS_PROXY=http://gateway:3128
      - http_proxy=http://gateway:3128
      - https_proxy=http://gateway:3128
{{EXTRA_ENV}}    networks:
      - internal
    depends_on:
      gateway:
        condition: service_healthy

{{USER}}{{VOLUMES}}    working_dir: /workarea
{{COMMAND}}
networks:
  external:
    driver: bridge
  internal:
    driver: bridge
    internal: true
"#;

/// Generate a squid.conf that only allows the given hostnames.
fn generate_squid_conf(allowed_hosts: &[&str]) -> String {
    let domain_acls: String = allowed_hosts
        .iter()
        .map(|host| format!("acl allowed_domains dstdomain {host}"))
        .collect::<Vec<_>>()
        .join("\n");

    SQUID_CONF_TEMPLATE.replace("{{DOMAIN_ACLS}}", &domain_acls)
}

/// Generate a compose.yaml for network-isolated mode.
fn generate_compose_yml(
    image_tag: &str,
    squid_conf_path: &str,
    mounts: &[(String, String)],
    user: Option<&str>,
    args: &[String],
) -> String {
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

    let user_line = match user {
        Some(u) => format!("    user: \"{u}\"\n"),
        None => String::new(),
    };

    let extra_env = match crate::auth::load_token() {
        Some(token) => format!("      - CLAUDE_API_KEY={token}\n"),
        None => String::new(),
    };

    COMPOSE_TEMPLATE
        .replace("{{SQUID_CONF_PATH}}", squid_conf_path)
        .replace("{{IMAGE_TAG}}", image_tag)
        .replace("{{USER}}", &user_line)
        .replace("{{VOLUMES}}", &volumes)
        .replace("{{EXTRA_ENV}}", &extra_env)
        .replace("{{COMMAND}}", &command)
}

/// Write out the compose project files to `dir` and return the compose file path.
pub fn write_compose_project(
    dir: &Path,
    image_tag: &str,
    extra_hosts: &[String],
    mounts: &[(String, String)],
    user: Option<&str>,
    args: &[String],
) -> Result<std::path::PathBuf, String> {
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
        f.write_all(generate_compose_yml(image_tag, "./squid.conf", mounts, user, args).as_bytes())
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
    fn test_compose_yml_structure() {
        let yml = generate_compose_yml(
            "claude-container:ubuntu-24.04",
            "/tmp/squid.conf",
            &[("/home/user/.claude".into(), "/root/.claude".into())],
            None,
            &[],
        );
        assert!(yml.contains("gateway:"));
        assert!(yml.contains("claude:"));
        assert!(yml.contains("internal: true"));
        assert!(yml.contains("HTTPS_PROXY=http://gateway:3128"));
    }
}
