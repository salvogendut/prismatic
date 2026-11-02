# Edit these defaults to build against another Iris fork or branch.
IRIS_REPO ?= https://github.com/salvogendut/iris.git
IRIS_BRANCH ?= xiri

# Omit Perfetto by default: some Iris revisions cannot compile it.
# Use FEATURES= for an interpreter build. Never enable developer for speed builds.
FEATURES ?= pcap,idle-pause,jitv2
PROFILE ?= release
CARGO ?= cargo
PYTHON ?= python3
ARGS ?=

export IRIS_REPO IRIS_BRANCH FEATURES PROFILE CARGO ARGS
.DEFAULT_GOAL := build
.PHONY: build fetch run check help test-bootstrap

build fetch run check:
	$(PYTHON) tools/build.py $@

help:
	$(PYTHON) tools/build.py --help

test-bootstrap:
	$(PYTHON) -m unittest discover -s tools -p 'test_build.py' -v
