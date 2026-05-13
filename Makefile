.PHONY: build-docker build install clean

PYTHON_VERSION := $(shell python3 -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')")
VARIANT := debian
IMAGE_NAME := rs-xml2json-builder-$(VARIANT)-py$(PYTHON_VERSION)

build-docker:
	docker build --build-arg PYTHON_VERSION=$(PYTHON_VERSION) -f Dockerfile.$(VARIANT) -t $(IMAGE_NAME) .

build: build-docker
	mkdir -p dist
	@TAG=$$(git describe --tags --abbrev=0 2>/dev/null); \
	if [ -z "$$TAG" ]; then \
		echo "No git tag found; building with placeholder version 0.0.0"; \
		VERSION="0.0.0"; \
	elif [ "$$(git rev-parse HEAD)" = "$$(git rev-list -n 1 "$$TAG")" ] && [ -z "$$(git status --porcelain)" ]; then \
		VERSION="$$TAG"; \
	else \
		SHA=$$(git rev-parse --short HEAD); \
		DIRTY=""; [ -n "$$(git status --porcelain)" ] && DIRTY=".dirty"; \
		VERSION="$$TAG+g$$SHA$$DIRTY"; \
	fi; \
	echo "Setting version to $$VERSION"; \
	cp Cargo.toml Cargo.toml.bak; \
	trap 'mv Cargo.toml.bak Cargo.toml' EXIT INT TERM; \
	sed -i "0,/^version = \".*\"$$/s//version = \"$$VERSION\"/" Cargo.toml; \
	docker run --rm \
		--user $$(id -u):$$(id -g) \
		-e HOME=/tmp -e CARGO_HOME=/tmp/.cargo \
		-v $(CURDIR):/app $(IMAGE_NAME)

VENV := .venv

install: build
	test -d $(VENV) || python3 -m venv $(VENV)
	$(VENV)/bin/python -m ensurepip --upgrade 2>/dev/null || \
		(curl -sS https://bootstrap.pypa.io/get-pip.py | $(VENV)/bin/python)
	$(VENV)/bin/python -m pip install --force-reinstall dist/rs_xml2json-*cp$(subst .,,$(PYTHON_VERSION))*.whl

clean:
	rm -rf dist
