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

### Image caching

Built images are tagged as `claude-container:<sanitized-base>` where `/` and `:`
are replaced with `-`. On `run`, if the tag already exists locally the build is
skipped. Use `--rebuild` to force a fresh build.

### Volume mounts (on `run`)

| Host | Container |
|------|-----------|
| `~/.claude` | `/root/.claude` |
| `~/.claude.json` | `/root/.claude.json` |
| `$(pwd)` | `/workarea` (working dir) |
