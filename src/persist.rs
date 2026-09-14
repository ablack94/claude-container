use std::path::{Path, PathBuf};

/// Claude's state directory inside the container (HOME is /home/claude).
pub const CONTAINER_CLAUDE_DIR: &str = "/home/claude/.claude";

/// The project is always mounted at /workarea, so the container-side project
/// directory under ~/.claude/projects/ always has the same name.
const CONTAINER_WORKDIR: &str = "/workarea";

/// Directories under ~/.claude that are persisted wholesale.
const PERSISTED_DIRS: &[&str] = &["todos", "shell-snapshots"];

/// Files under ~/.claude that are persisted individually.
const PERSISTED_FILES: &[&str] = &["history.jsonl", "CLAUDE.md"];

/// Slugify a path the way Claude names directories under ~/.claude/projects:
/// every character outside [A-Za-z0-9] becomes '-'.
pub fn project_slug(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn path_str(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Path is not valid UTF-8: {}", path.display()))
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("Failed to create {}: {e}", path.display()))
}

/// Create an empty file if it doesn't exist. A bind mount whose host path is
/// missing makes the runtime create a *directory* there, which Claude then
/// fails to read as a file.
fn ensure_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            ensure_dir(parent)?;
        }
        std::fs::File::create(path)
            .map_err(|e| format!("Failed to create {}: {e}", path.display()))?;
    }
    Ok(())
}

/// Host path holding the chats and memories for a given project directory.
pub fn host_project_dir(home: &str, workdir: &str) -> PathBuf {
    PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(project_slug(workdir))
}

/// Build the (host, container) mounts that keep chats, memories and related
/// session state in the host's ~/.claude tree instead of the container tmpfs.
///
/// The project directory is remapped: host `~/.claude/projects/<slug of the
/// host workdir>` becomes the container's `~/.claude/projects/-workarea`, so
/// sessions started in the container line up with host sessions for the same
/// project rather than colliding under one shared `-workarea`.
///
/// `forwarding_settings` means the host's whole ~/.claude is already mounted,
/// so the shared state below persists on its own and only the project remap is
/// still needed.
pub fn persist_mounts(
    home: &str,
    workdir: &str,
    forwarding_settings: bool,
) -> Result<Vec<(String, String)>, String> {
    let host_claude = PathBuf::from(home).join(".claude");
    let host_project = host_project_dir(home, workdir);
    ensure_dir(&host_project)?;

    let mut mounts = vec![(
        path_str(&host_project)?,
        format!(
            "{CONTAINER_CLAUDE_DIR}/projects/{}",
            project_slug(CONTAINER_WORKDIR)
        ),
    )];

    if forwarding_settings {
        eprintln!("Persisting chats and memories to {}", host_project.display());
        return Ok(mounts);
    }

    for dir in PERSISTED_DIRS {
        let host = host_claude.join(dir);
        ensure_dir(&host)?;
        mounts.push((path_str(&host)?, format!("{CONTAINER_CLAUDE_DIR}/{dir}")));
    }

    for file in PERSISTED_FILES {
        let host = host_claude.join(file);
        ensure_file(&host)?;
        mounts.push((path_str(&host)?, format!("{CONTAINER_CLAUDE_DIR}/{file}")));
    }

    eprintln!("Persisting chats and memories to {}", host_project.display());
    Ok(mounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_slug_matches_claude_layout() {
        assert_eq!(project_slug("/workarea"), "-workarea");
        assert_eq!(
            project_slug("/home/ablack94/claude-container"),
            "-home-ablack94-claude-container"
        );
        assert_eq!(project_slug("/home/me/src/foo.bar"), "-home-me-src-foo-bar");
    }

    #[test]
    fn test_persist_mounts_layout() {
        let tmp = std::env::temp_dir().join(format!("cc-persist-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let home = tmp.to_str().unwrap().to_string();

        let mounts = persist_mounts(&home, "/home/me/proj", false).unwrap();

        let project = mounts
            .iter()
            .find(|(_, c)| c == "/home/claude/.claude/projects/-workarea")
            .expect("project dir is mounted");
        assert!(project.0.ends_with(".claude/projects/-home-me-proj"));
        assert!(std::path::Path::new(&project.0).is_dir());

        for container in [
            "/home/claude/.claude/todos",
            "/home/claude/.claude/shell-snapshots",
            "/home/claude/.claude/history.jsonl",
            "/home/claude/.claude/CLAUDE.md",
        ] {
            assert!(
                mounts.iter().any(|(_, c)| c == container),
                "missing mount for {container}"
            );
        }

        // Files must exist as files so the runtime doesn't create directories.
        assert!(tmp.join(".claude/history.jsonl").is_file());
        assert!(tmp.join(".claude/CLAUDE.md").is_file());

        // With ~/.claude already forwarded, only the project remap is needed.
        let forwarded = persist_mounts(&home, "/home/me/proj", true).unwrap();
        assert_eq!(forwarded.len(), 1);
        assert_eq!(forwarded[0].1, "/home/claude/.claude/projects/-workarea");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
