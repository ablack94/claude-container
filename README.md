# claude-container

Containerized wrapper for `@anthropic-ai/claude-code` with a lightweight CLI
script and offline-friendly image packaging.

## Build

```sh
docker build -t claude-container:latest .
```

## Bundle the image (tarball)

```sh
docker save -o claude-container.tar claude-container:latest
```

On another machine:

```sh
docker load -i claude-container.tar
```

## Run

Use the wrapper script to ensure `~/.claude` is bind-mounted into the container:

```sh
./bin/claude-container --help
```

Or run directly:

```sh
docker run --rm -it \
  -v "$HOME/.claude:/home/node/.claude" \
  claude-container:latest \
  --help
```
