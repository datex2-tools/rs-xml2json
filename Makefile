.PHONY: build-docker build install clean

PYTHON_VERSION := $(shell python3 -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')")
VARIANT := debian
IMAGE_NAME := xml2json-builder-$(VARIANT)-py$(PYTHON_VERSION)

build-docker:
	docker build --build-arg PYTHON_VERSION=$(PYTHON_VERSION) -f Dockerfile.$(VARIANT) -t $(IMAGE_NAME) .

build: build-docker
	mkdir -p dist
	docker run --rm -v $(CURDIR):/app $(IMAGE_NAME)

VENV := .venv

install: build
	test -d $(VENV) || python3 -m venv $(VENV)
	$(VENV)/bin/python -m ensurepip --upgrade 2>/dev/null || \
		(curl -sS https://bootstrap.pypa.io/get-pip.py | $(VENV)/bin/python)
	$(VENV)/bin/python -m pip install --force-reinstall dist/xml2json-*cp$(subst .,,$(PYTHON_VERSION))*.whl

clean:
	rm -rf dist
