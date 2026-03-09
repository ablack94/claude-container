FROM {{BASE_IMAGE}}
COPY --from={{CLAUDE_SOURCE_IMAGE}} /usr/local/bin/claude /usr/local/bin/claude
RUN existing_group=$(getent group {{GID}} | cut -d: -f1) && \
    existing_user=$(getent passwd {{UID}} | cut -d: -f1) && \
    if command -v groupadd >/dev/null 2>&1; then \
      [ -z "$existing_user" ] || userdel "$existing_user"; \
      [ -z "$existing_group" ] || groupdel "$existing_group"; \
      groupadd -g {{GID}} claude && \
      useradd -m -u {{UID}} -g {{GID}} -d /home/claude -s /bin/bash claude; \
    else \
      [ -z "$existing_user" ] || deluser "$existing_user"; \
      [ -z "$existing_group" ] || delgroup "$existing_group"; \
      addgroup -g {{GID}} claude && \
      adduser -u {{UID}} -G claude -h /home/claude -s /bin/sh -D claude; \
    fi
USER claude
WORKDIR /home/claude
RUN CLAUDE_VERSION=$(claude --version 2>/dev/null | head -1 | sed 's/[^0-9.]//g' || echo "0.0.0") && \
    mkdir -p /home/claude/.claude && \
    printf '%s' '{"hasCompletedOnboarding":true,"lastOnboardingVersion":"'"$CLAUDE_VERSION"'","numStartups":1,"projects":{"/workarea":{"hasTrustDialogAccepted":true,"projectOnboardingSeenCount":1,"allowedTools":[],"mcpContextUris":[],"mcpServers":{},"enabledMcpjsonServers":[],"disabledMcpjsonServers":[],"hasClaudeMdExternalIncludesApproved":false,"hasClaudeMdExternalIncludesWarningShown":false}}}' > /home/claude/.claude.json && \
    printf '%s' '{"skipDangerousModePermissionPrompt":true}' > /home/claude/.claude/settings.json
CMD ["/usr/local/bin/claude", "--dangerously-skip-permissions"]
