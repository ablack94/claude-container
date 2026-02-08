# claude-container

Containerized wrapper for `@anthropic-ai/claude-code` with a lightweight CLI
script and offline-friendly image packaging.

## Build

```sh
docker build -t claude-container:latest .
```

Or via Make:

```sh
make build
```

## Install

After building, install the `claude-container` wrapper script to your system:

```sh
sudo make install
```

This installs to `/usr/local/bin/claude-container` by default. You can override the install location:

```sh
# Install to /usr/bin
sudo make install PREFIX=/usr

# Install to ~/.local/bin (no sudo needed)
make install PREFIX=$HOME/.local
```

To uninstall:

```sh
sudo make uninstall
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

You can also use Make:

```sh
make run ARGS="--help"
```
