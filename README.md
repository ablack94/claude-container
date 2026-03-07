# claude-container

Rust CLI tool that builds and runs Claude in any container image. Generates a
Dockerfile on the fly, layers the Claude binary from
`ghcr.io/ablack94/docker-claude:stable` onto your chosen base image, caches the
result, and runs it with appropriate mounts. Supports both Docker and Podman.

## Build

```sh
cargo build --release
```

The binary is at `target/release/claude-container`.

## Usage

### Authentication

Set up a long-lived API token. This runs `claude setup-token` under the hood,
captures the token, and stores it at `~/.claude-container/token`. The token is
automatically injected as `CLAUDE_API_KEY` on subsequent runs.

```sh
claude-container auth
```

### Run Claude in a container

```sh
# Build (if needed) and launch — auto-detects docker/podman
claude-container run ubuntu:24.04

# Pass arguments to Claude
claude-container run ubuntu:24.04 -- --help

# Force rebuild
claude-container run ubuntu:24.04 --rebuild

# Explicit runtime
claude-container --runtime podman run ubuntu:24.04

# Forward host ~/.claude and ~/.claude.json into the container
claude-container run ubuntu:24.04 --forward-settings
```

### Build image only

```sh
# Build and tag as claude-container:ubuntu-24.04
claude-container build ubuntu:24.04

# Custom tag
claude-container build ubuntu:24.04 --tag my-claude:latest

# Force rebuild
claude-container build ubuntu:24.04 --rebuild
```

### Network isolation

Run Claude in a network-isolated container where only whitelisted hosts are
reachable. Traffic is routed through a squid proxy gateway; everything else is
blocked. `*.anthropic.com` and `*.claude.com` are always allowed.

```sh
# Isolated mode — only *.anthropic.com and *.claude.com are reachable
claude-container run ubuntu:24.04 --isolated

# Allow additional hosts
claude-container run ubuntu:24.04 --allow-host github.com --allow-host pypi.org

# --allow-host implies --isolated
claude-container run ubuntu:24.04 --allow-host npmjs.org
```

Architecture:
```
[host network] <-> [squid proxy] <-> [internal network] <-> [claude container]
```

The claude container has no direct internet access. The squid proxy sits on both
networks and only forwards requests to whitelisted hostnames.

### Compose workflow

Render and manage compose project files locally for debugging, customization,
or manual execution. Files are written to `.claude-container/` by default.

```sh
# Render compose files to ./.claude-container/
claude-container compose render ubuntu:24.04

# Allow additional hosts
claude-container compose render ubuntu:24.04 --allow-host github.com

# Run as a specific user inside the claude container
claude-container compose render ubuntu:24.04 --user 1000:1000

# Forward host settings
claude-container compose render ubuntu:24.04 --forward-settings

# Custom output directory
claude-container compose render ubuntu:24.04 -o my-compose

# Run a previously rendered compose project
claude-container compose run

# Run from a custom directory
claude-container compose run -d my-compose
```

This writes `compose.yaml` and `squid.conf` to the output directory. You can
inspect and edit these files before running. The `compose run` subcommand
handles startup and teardown automatically.

### Image caching

Built images are tagged as `claude-container:<sanitized-base>` where `/` and `:`
are replaced with `-`. On `run`, if the tag already exists locally the build is
skipped. Use `--rebuild` to force a fresh build.

### Volume mounts (on `run`)

| Host | Container | When |
|------|-----------|------|
| `$(pwd)` | `/workarea` (working dir) | Always |
| `~/.claude` | `/root/.claude` | `--forward-settings` |
| `~/.claude.json` | `/root/.claude.json` | `--forward-settings` |
