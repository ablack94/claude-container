use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const TOKEN_DIR: &str = ".claude-container";
const TOKEN_FILE: &str = "token";

/// Return the path to ~/.claude-container/token.
fn token_path() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "HOME environment variable not set".to_string())?;
    Ok(std::path::PathBuf::from(home).join(TOKEN_DIR).join(TOKEN_FILE))
}

/// Read the stored token, if any.
pub fn load_token() -> Option<String> {
    let path = token_path().ok()?;
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Save a token string to ~/.claude-container/token with 0600 permissions.
fn save_token(token: &str) -> Result<(), String> {
    let path = token_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("Failed to write token file: {e}"))?;

    file.write_all(token.as_bytes())
        .map_err(|e| format!("Failed to write token: {e}"))?;

    eprintln!("Token saved to {}", path.display());
    Ok(())
}

/// Scan a stream for URLs and tokens, updating shared state.
fn scan_stream<R: std::io::Read>(
    reader: R,
    token: &Arc<Mutex<Option<String>>>,
) {
    let reader = BufReader::new(reader);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();

        if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
            eprintln!("Open this URL to authenticate:\n\n  {trimmed}\n");
        }

        if trimmed.starts_with("sk-") {
            *token.lock().unwrap() = Some(trimmed.to_string());
        }
    }
}

/// Run `claude setup-token`, intercept output to extract the token, and store it.
pub fn setup_token() -> Result<(), String> {
    eprintln!("Starting authentication...\n");

    let mut child = Command::new("claude")
        .arg("setup-token")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run `claude setup-token`: {e}"))?;

    let token: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let stdout = child.stdout.take().unwrap();
    let token_for_stdout = Arc::clone(&token);
    let stdout_thread = std::thread::spawn(move || {
        scan_stream(stdout, &token_for_stdout);
    });

    let stderr = child.stderr.take().unwrap();
    let token_for_stderr = Arc::clone(&token);
    let stderr_thread = std::thread::spawn(move || {
        scan_stream(stderr, &token_for_stderr);
    });

    let status = child.wait().map_err(|e| format!("Failed to wait for process: {e}"))?;
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();

    if !status.success() {
        return Err(format!("`claude setup-token` exited with code {:?}", status.code()));
    }

    let token = token.lock().unwrap().take();

    match token {
        Some(t) => save_token(&t),
        None => {
            eprintln!("Could not detect token automatically.");
            eprintln!("Paste your token (sk-...) and press Enter:");
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .map_err(|e| format!("Failed to read token: {e}"))?;
            let input = input.trim();
            if !input.starts_with("sk-") {
                return Err("Invalid token. Expected a value starting with `sk-`.".to_string());
            }
            save_token(input)
        }
    }
}
