IMAGE_NAME ?= claude-container:latest
TARBALL ?= claude-container.tar

.PHONY: build bundle load run

build:
  docker build -t $(IMAGE_NAME) .

bundle: build
  docker save -o $(TARBALL) $(IMAGE_NAME)

load:
  docker load -i $(TARBALL)

run:
  docker run --rm -it \
    -v "$(HOME)/.claude:/home/node/.claude" \
    $(IMAGE_NAME) \
    $(ARGS)
