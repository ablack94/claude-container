FROM {{BASE_IMAGE}}
COPY --from={{CLAUDE_SOURCE_IMAGE}} /usr/local/bin/claude /usr/local/bin/claude
COPY entrypoint.sh /entrypoint.sh
# The container runs as the host's UID:GID, which need not exist in the base
# image. The Dev Containers extension runs `id -un` on connect and hard-fails
# when the runtime UID has no /etc/passwd entry, so synthesize one if missing.
# `getent` is absent from some minimal images; fall back to matching the id
# field directly. POSIX sh only — this runs under the image's /bin/sh.
RUN { command -v getent >/dev/null 2>&1 && getent group {{GID}} >/dev/null 2>&1; } \
    || grep -q "^[^:]*:[^:]*:{{GID}}:" /etc/group 2>/dev/null \
    || echo "claude:x:{{GID}}:" >> /etc/group
RUN { command -v getent >/dev/null 2>&1 && getent passwd {{UID}} >/dev/null 2>&1; } \
    || grep -q "^[^:]*:[^:]*:{{UID}}:" /etc/passwd 2>/dev/null \
    || echo "claude:x:{{UID}}:{{GID}}::/home/claude:/bin/sh" >> /etc/passwd
WORKDIR /workarea
ENTRYPOINT ["/entrypoint.sh"]
CMD ["--dangerously-skip-permissions"]
