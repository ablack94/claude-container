#!/bin/sh
set -e

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

exec claude "$@"
