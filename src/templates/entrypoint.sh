#!/bin/sh
set -e

# Dev Containers sentinel: initialize as usual, then idle so the container stays
# up under `compose up -d` instead of exiting when Claude exits.
IDLE=0
if [ "$1" = "devcontainer-idle" ]; then
    IDLE=1
    shift
fi

# Initialize Claude config in the home directory (tmpfs mount, writable by any user)
CLAUDE_VERSION=$(claude --version 2>/dev/null | head -1 | sed 's/[^0-9.]//g' || echo "0.0.0")

mkdir -p "$HOME/.claude"

# Only write defaults if config doesn't exist (e.g. not forwarded from host)
if [ ! -f "$HOME/.claude.json" ]; then
    printf '%s' '{"hasCompletedOnboarding":true,"lastOnboardingVersion":"'"$CLAUDE_VERSION"'","numStartups":1,"projects":{"/workarea":{"hasTrustDialogAccepted":true,"projectOnboardingSeenCount":1,"allowedTools":[],"mcpContextUris":[],"mcpServers":{},"enabledMcpjsonServers":[],"disabledMcpjsonServers":[],"hasClaudeMdExternalIncludesApproved":false,"hasClaudeMdExternalIncludesWarningShown":false}}}' > "$HOME/.claude.json"
fi

if [ ! -f "$HOME/.claude/settings.json" ]; then
    printf '%s' '{"skipDangerousModePermissionPrompt":true}' > "$HOME/.claude/settings.json"
fi

if [ "$IDLE" = "1" ]; then
    # Portable idle: an unbounded `sleep` argument is a GNU coreutils extension
    # that busybox rejects, so loop over finite sleeps instead. Compose sets
    # `init: true`, so the init process reaps and forwards signals.
    while :; do sleep 3600; done
fi

exec claude "$@"
