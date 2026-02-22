use std::io::Write;
use std::process::Command;

use crate::runtime::Runtime;

const CLAUDE_SOURCE_IMAGE: &str = "ghcr.io/ablack94/docker-claude:stable";

/// Sanitize a base image name into a valid tag suffix.
/// `ubuntu:24.04` → `ubuntu-24.04`, `ghcr.io/foo/bar:latest` → `ghcr.io-foo-bar-latest`
pub fn sanitize_tag(base_image: &str) -> String {
    let sanitized: String = base_image
        .chars()
        .map(|c| match c {
            '/' | ':' => '-',
            c => c,
        })
        .collect();
    format!("claude-container:{sanitized}")
}

/// Check if an image exists locally.
pub fn image_exists(runtime: Runtime, tag: &str) -> bool {
    Command::new(runtime.cmd())
        .args(["image", "inspect", tag])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Generate a Dockerfile as a String.
fn generate_dockerfile(base_image: &str) -> String {
    format!(
        "FROM {base_image}\n\
         COPY --from={CLAUDE_SOURCE_IMAGE} /usr/local/bin/claude /usr/local/bin/claude\n\
         CMD [\"/usr/local/bin/claude\", \"--dangerously-skip-permissions\"]\n"
    )
}

/// Build the image, using cache unless `rebuild` is set. Returns the final tag.
pub fn ensure_image(
    runtime: Runtime,
    base_image: &str,
    custom_tag: Option<&str>,
    rebuild: bool,
) -> Result<String, String> {
    let tag = custom_tag
        .map(|t| t.to_string())
        .unwrap_or_else(|| sanitize_tag(base_image));

    if !rebuild && image_exists(runtime, &tag) {
        eprintln!("Image {} found locally, skipping build.", tag);
        return Ok(tag);
    }

    eprintln!("Building image {} from base {}...", tag, base_image);

    let tmp_dir = tempfile::tempdir()
        .map_err(|e| format!("Failed to create temp dir: {e}"))?;
    let dockerfile_path = tmp_dir.path().join("Dockerfile");

    {
        let mut f = std::fs::File::create(&dockerfile_path)
            .map_err(|e| format!("Failed to write Dockerfile: {e}"))?;
        f.write_all(generate_dockerfile(base_image).as_bytes())
            .map_err(|e| format!("Failed to write Dockerfile: {e}"))?;
    }

    let status = Command::new(runtime.cmd())
        .args([
            "build",
            "-t",
            &tag,
            "-f",
            dockerfile_path.to_str().unwrap(),
            tmp_dir.path().to_str().unwrap(),
        ])
        .status()
        .map_err(|e| format!("Failed to run {} build: {e}", runtime.cmd()))?;

    if !status.success() {
        return Err(format!("{} build failed with exit code {:?}", runtime.cmd(), status.code()));
    }

    eprintln!("Successfully built {}.", tag);
    Ok(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_tag() {
        assert_eq!(sanitize_tag("ubuntu:24.04"), "claude-container:ubuntu-24.04");
        assert_eq!(sanitize_tag("ghcr.io/foo/bar:latest"), "claude-container:ghcr.io-foo-bar-latest");
        assert_eq!(sanitize_tag("alpine"), "claude-container:alpine");
    }
}
