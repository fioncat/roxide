COMMIT := $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
COMMIT_FULL := $(shell git rev-parse HEAD 2>/dev/null || echo "unknown")
DIRTY := $(shell git status --porcelain 2>/dev/null | wc -l | tr -d ' ')
CURRENT_TAG := $(shell git describe --exact-match --tags 2>/dev/null || echo "")
LATEST_TAG := $(shell git describe --tags --abbrev=0 2>/dev/null || echo "")

ifeq ($(CURRENT_TAG),)
  ifeq ($(LATEST_TAG),)
    VERSION := dev_$(COMMIT)
  else
    VERSION := $(LATEST_TAG)_dev_$(COMMIT)
  endif
else
  VERSION := $(CURRENT_TAG)
endif

ifneq ($(DIRTY),0)
  VERSION := $(VERSION)_dirty
endif

TIME := $(shell date -u +"%Y-%m-%dT%H:%M:%SZ")

PKG_VERSION := github.com/fioncat/roxide/build

LDFLAGS := -ldflags "-X $(PKG_VERSION).Version=$(VERSION) \
                     -X $(PKG_VERSION).Commit=$(COMMIT_FULL) \
                     -X $(PKG_VERSION).Time=$(TIME)"

DOCKER_CMD ?= docker
BUILD_IMAGE ?= golang:1-alpine
BASE_IMAGE ?= alpine:latest
TARGET_IMAGE ?= fioncat/roxide:$(VERSION)

.PHONY: build
build:
	CGO_ENABLED=1 CGO_CFLAGS="-D_LARGEFILE64_SOURCE" go build \
		$(LDFLAGS) \
		-o ./bin/roxide \
		./main.go

.PHONY: install
install:
	CGO_ENABLED=1 CGO_CFLAGS="-D_LARGEFILE64_SOURCE" go install \
		$(LDFLAGS)

.PHONY: upgrade
upgrade:
	@go get -u && go mod tidy

.PHONY: test
test:
	CGO_ENABLED=1 CGO_CFLAGS="-D_LARGEFILE64_SOURCE" go test ./...

.PHONY: docker
docker:
	$(DOCKER_CMD) build \
		--build-arg BUILD_IMAGE=$(BUILD_IMAGE) \
		--build-arg BASE_IMAGE=$(BASE_IMAGE) \
		-t $(TARGET_IMAGE) \
		.

.PHONY: info
info:
	@echo "Version: $(VERSION)"
	@echo "Commit: $(COMMIT_FULL)"
