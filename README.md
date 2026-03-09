# claude-container

Rust CLI tool that builds and runs Claude in any container image. Generates a
Dockerfile on the fly, layers the Claude binary from
`ghcr.io/ablack94/docker-claude:stable` onto your chosen base image, and runs
it via Docker Compose. Supports both Docker and Podman.

## Build

```sh
cargo build --release
```

The binary is at `target/release/claude-container`.

## Usage

### Authentication

Authentication is managed through named profiles stored at
`~/.config/claude-container/profiles/<name>.env`. Each profile is a secure env
file (mode 0600) referenced by the generated compose file — secrets are never
inlined in `compose.yaml`.

**Create an OAuth profile** (runs `claude setup-token` under the hood):

```sh
claude-container auth create work
```

**Create an API key profile:**

```sh
claude-container auth create personal --api-key
```

**Set a default profile:**

```sh
claude-container auth default work
```

**List profiles:**

```sh
claude-container auth list
```

**Remove a profile:**

```sh
claude-container auth remove old-profile
```

If `ANTHROPIC_API_KEY` is set in the host environment, it is also passed through
to the container regardless of which profile is active.

When no `--profile` is specified on `build`, the default profile is used. If no
default is set and no `ANTHROPIC_API_KEY` is in the environment, a warning is
printed.

### Build a project

Generates a `.claude-container/` directory in the current working directory
containing a `Dockerfile` and `compose.yaml`.

```sh
# Generate .claude-container/ project files
claude-container build ubuntu:24.04

# Build and immediately run
claude-container build ubuntu:24.04 --run

# Use a specific auth profile
claude-container build ubuntu:24.04 --profile personal --run

# Pass extra arguments to Claude
claude-container build ubuntu:24.04 -- -p "hello"

# Forward host ~/.claude and ~/.claude.json into the container
claude-container build ubuntu:24.04 --forward-settings

# Explicit runtime
claude-container --runtime podman build ubuntu:24.04
```

### Run an existing project

```sh
# Run the .claude-container/ project in the current directory
claude-container run

# Force rebuild of the container image
claude-container run --rebuild
```

### Network isolation

Run Claude in a network-isolated container where only whitelisted hosts are
reachable. Traffic is routed through a squid proxy gateway; everything else is
blocked. `*.anthropic.com` and `*.claude.com` are always allowed.

```sh
# Isolated mode — only *.anthropic.com and *.claude.com are reachable
claude-container build ubuntu:24.04 --isolated --run

# Allow additional hosts (implies --isolated)
claude-container build ubuntu:24.04 --allow-host github.com --allow-host pypi.org --run
```

Architecture:
```
[host network] <-> [squid proxy] <-> [internal network] <-> [claude container]
```

The claude container has no direct internet access. The squid proxy sits on both
networks and only forwards requests to whitelisted hostnames.

### Runtime configuration

By default the CLI auto-detects Docker or Podman. You can set a default runtime
and ban runtimes you don't want used:

```sh
# Set podman as the default
claude-container config runtime podman

# Ban docker entirely
claude-container config ban docker

# Show current config
claude-container config show

# Remove a ban
claude-container config ban docker --remove

# Clear the default (revert to auto-detect)
claude-container config runtime --clear
```

The `--runtime` flag on any command overrides the configured default, but banned
runtimes are always rejected. Configuration is stored at
`~/.config/claude-container/config`.

### The `-C` flag

Works like `git -C` or `make -C` — changes the working directory before doing
anything else. Both `build` and `run` then operate relative to that directory:

```sh
# Build a project rooted at /path/to/project
claude-container -C /path/to/project build ubuntu:24.04

# Run an existing project in another directory
claude-container -C /path/to/project run
```

### Volume mounts

| Host | Container | When |
|------|-----------|------|
| `$(pwd)` | `/workarea` (working dir) | Always |
| `~/.claude` | `/home/claude/.claude` | `--forward-settings` |
| `~/.claude.json` | `/home/claude/.claude.json` | `--forward-settings` |
