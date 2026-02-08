IMAGE_NAME ?= claude-container:latest
TARBALL ?= claude-container.tar
PREFIX ?= /usr/local

.PHONY: build bundle load run install uninstall

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

install:
	install -D -m 755 bin/claude-container $(DESTDIR)$(PREFIX)/bin/claude-container

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/claude-container
