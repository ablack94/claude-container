FROM {{BASE_IMAGE}}
COPY --from={{CLAUDE_SOURCE_IMAGE}} /usr/local/bin/claude /usr/local/bin/claude
COPY entrypoint.sh /entrypoint.sh
WORKDIR /workarea
ENTRYPOINT ["/entrypoint.sh"]
CMD ["--dangerously-skip-permissions"]
