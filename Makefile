IMAGE_NAME ?= claude-container:latest
TARBALL ?= claude-container.tar

.PHONY: build bundle load run

build:
\tdocker build -t $(IMAGE_NAME) .

bundle: build
\tdocker save -o $(TARBALL) $(IMAGE_NAME)

load:
\tdocker load -i $(TARBALL)

run:
\tdocker run --rm -it \
\t\t-v "$(HOME)/.claude:/home/node/.claude" \
\t\t$(IMAGE_NAME) \
\t\t$(ARGS)
