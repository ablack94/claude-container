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

**Create an OAuth profile:**

```sh
claude-container auth create work oauth <token>
```

**Create an API key profile:**

```sh
claude-container auth create personal api-key <key>
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

# Persist chats and memories to the host ~/.claude tree
claude-container build ubuntu:24.04 --persist --run

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

### VS Code Dev Containers

`--devcontainer` additionally generates a VS Code Dev Containers setup that
launches the same compose stack, so you can work inside the container with full
VS Code integration:

```sh
# Generate .devcontainer/devcontainer.json alongside .claude-container/
claude-container build ubuntu:24.04 --devcontainer

# Install extra VS Code extensions in the container (implies --devcontainer)
claude-container build ubuntu:24.04 --vscode-extension ms-python.python \
    --vscode-extension rust-lang.rust-analyzer
```

Two files are written:

| File | Purpose |
|------|---------|
| `.devcontainer/devcontainer.json` | What VS Code reads — points at both compose files |
| `.claude-container/compose.devcontainer.yaml` | Compose override that keeps the container idling |

Then run **Dev Containers: Reopen in Container** from the VS Code command
palette. The project is opened at `/workarea`, the same path the normal
`claude-container run` flow uses.

Claude does **not** start automatically: normally the container's entrypoint
execs `claude` and the container exits when Claude does, which Dev Containers
cannot work with. The override passes a `devcontainer-idle` sentinel to the
entrypoint, which performs the usual Claude config initialization and then
idles. To use Claude, open a terminal in VS Code and run:

```sh
claude --dangerously-skip-permissions
```

The Dockerfile's `CMD` supplies `--dangerously-skip-permissions` to the
entrypoint, so `claude-container run` effectively runs
`claude --dangerously-skip-permissions`. Pass the flag yourself in the VS Code
terminal to match that behaviour, or drop it if you would rather be prompted.

The `anthropic.claude-code` extension is installed in the container by default;
any `--vscode-extension` IDs are added to it. Credentials and environment come
from exactly the same `env_file` mechanism as the regular compose flow, so the
auth profile selected with `--profile` (or the default profile, or
`ANTHROPIC_API_KEY`) is available inside the dev container.

Some caveats worth knowing:

- **The compose files are not committable.** `.claude-container/` is gitignored,
  but `devcontainer.json` points into it. If you commit
  `.devcontainer/devcontainer.json`, every fresh clone still has to run
  `claude-container build <image> --devcontainer` once before **Reopen in
  Container** will work.
- **`--isolated` blocks the extension marketplace.** The egress proxy only
  allows Anthropic hosts, so VS Code's attempts to install extensions in the
  container will fail unless you add `--allow-host` entries for the marketplace
  domains.
- **Podman needs pointing at.** The Dev Containers extension drives `docker` by
  default; podman users must tell VS Code otherwise (e.g. the
  `dev.containers.dockerPath` setting).
- **Trailing `-- <args>` do not reach dev container sessions.** They become the
  compose `command:`, which the dev container override replaces; they apply to
  `claude-container run` only.

Generated `devcontainer.json` files start with a `// Generated by
claude-container` marker comment (`devcontainer.json` is JSONC, so comments are
legal). A build refuses to overwrite a `devcontainer.json` without that marker,
and `claude-container clean` removes the file only if the marker is present —
then deletes `.devcontainer/` only if it is left empty.

### Persisting chats and memories

By default the container's home directory is a tmpfs, so transcripts, memories
and prompt history die with the container. `--persist` keeps them on the host
inside your regular `~/.claude` tree:

```sh
# Persist this session's chats and memories
claude-container build ubuntu:24.04 --persist --run

# Persist by default for every build
claude-container config persist true

# Opt out for a single build
claude-container build ubuntu:24.04 --no-persist
```

The project is always mounted at `/workarea` inside the container, so every
container would otherwise write to the same `~/.claude/projects/-workarea`
directory. Instead the project directory is remapped to the slug of the *host*
path, which is what the host's own Claude Code uses:

```
~/.claude/projects/-home-you-code-myproj/   (host)
        |
        +-- <session-id>.jsonl   chats
        +-- memory/              memories
        |
        v  bind mount
/home/claude/.claude/projects/-workarea/    (container)
```

Because the naming lines up, sessions run in the container show up in
`claude --resume` on the host for that same project, and memories are shared
between them.

Also persisted: `~/.claude/history.jsonl` (prompt history), `~/.claude/CLAUDE.md`
(user-level memory — created empty if missing), `~/.claude/todos/` and
`~/.claude/shell-snapshots/`. Everything else in the container home stays
ephemeral. With `--forward-settings` the whole `~/.claude` is already mounted,
so `--persist` only adds the project remap.

Resolution order for the setting: `--no-persist` > `--persist` >
`config persist` > off.

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

# Persist chats and memories by default
claude-container config persist true

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
| `~/.gitconfig` | `/home/claude/.gitconfig` (ro) | `--forward-git-config` |
| `~/.claude/projects/<host slug>` | `/home/claude/.claude/projects/-workarea` | `--persist` |
| `~/.claude/history.jsonl` | `/home/claude/.claude/history.jsonl` | `--persist` |
| `~/.claude/CLAUDE.md` | `/home/claude/.claude/CLAUDE.md` | `--persist` |
| `~/.claude/todos` | `/home/claude/.claude/todos` | `--persist` |
| `~/.claude/shell-snapshots` | `/home/claude/.claude/shell-snapshots` | `--persist` |
