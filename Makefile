# Edit these defaults to build against another Iris fork or branch.
IRIS_REPO ?= https://github.com/salvogendut/iris.git
IRIS_BRANCH ?= xiri
INSTIGATOR_REPO ?= https://github.com/jamesbraid/instigator.git
INSTIGATOR_BRANCH ?= main

# Omit Perfetto by default: some Iris revisions cannot compile it.
# Use FEATURES= for an interpreter build. Never enable developer for speed builds.
FEATURES ?= pcap,idle-pause,jitv2
PROFILE ?= release
CARGO ?= cargo
PYTHON ?= python3
ARGS ?=

export IRIS_REPO IRIS_BRANCH INSTIGATOR_REPO INSTIGATOR_BRANCH FEATURES PROFILE CARGO ARGS
.DEFAULT_GOAL := build
.PHONY: build fetch run check help test-bootstrap flatpak flatpak-prepare install-server-image install-server

build fetch run check:
	$(PYTHON) tools/build.py $@

help:
	$(PYTHON) tools/build.py --help

test-bootstrap:
	$(PYTHON) -m unittest discover -s tools -p 'test_build.py' -v

# Uses the same editable Iris repository/branch defaults; sources and bundle
# are isolated under target/flatpak. See docs/flatpak.md for SDK prerequisites.
flatpak:
	$(PYTHON) tools/flatpak.py build

flatpak-prepare:
	$(PYTHON) tools/flatpak.py prepare

# Alternate Linux container backend. Only this target requires rootless Podman.
install-server-image:
	$(PYTHON) tools/install_server.py image

install-server:
	$(PYTHON) tools/install_server.py native
