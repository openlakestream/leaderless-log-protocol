# Makefile for local verification of formal specifications

TLA2TOOLS_VERSION := v1.8.0
TLA2TOOLS_URL := https://github.com/tlaplus/tlaplus/releases/download/$(TLA2TOOLS_VERSION)/tla2tools.jar
TLA2TOOLS_JAR := tlaplus/tla2tools.jar

FIZZBEE_VERSION := v0.4.0
FIZZ_DIR := $(HOME)/.local/fizzbee

# Detect platform
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

ifeq ($(UNAME_S),Darwin)
  FIZZBEE_OS := macos
else
  FIZZBEE_OS := linux
endif

ifeq ($(UNAME_M),x86_64)
  FIZZBEE_ARCH := x86
else ifeq ($(UNAME_M),aarch64)
  FIZZBEE_ARCH := arm
else ifeq ($(UNAME_M),arm64)
  FIZZBEE_ARCH := arm
else
  $(error Unsupported architecture: $(UNAME_M))
endif

FIZZBEE_TARBALL := fizzbee-$(FIZZBEE_VERSION)-$(FIZZBEE_OS)_$(FIZZBEE_ARCH).tar.gz
FIZZBEE_URL := https://github.com/fizzbee-io/fizzbee/releases/download/$(FIZZBEE_VERSION)/$(FIZZBEE_TARBALL)
FIZZ_BIN := $(FIZZ_DIR)/fizz

.PHONY: fizzbee-install tlaplus-install fizzbee tlaplus verify check clean

## fizzbee-install: Download and install Fizzbee CLI locally
fizzbee-install:
	@echo "Installing Fizzbee $(FIZZBEE_VERSION) for $(FIZZBEE_OS)/$(FIZZBEE_ARCH)..."
	@mkdir -p $(FIZZ_DIR)
	@curl -fsSL -o /tmp/fizzbee.tar.gz "$(FIZZBEE_URL)"
	@tar xzf /tmp/fizzbee.tar.gz -C /tmp/fizzbee-extract 2>/dev/null || \
		(mkdir -p /tmp/fizzbee-extract && tar xzf /tmp/fizzbee.tar.gz -C /tmp/fizzbee-extract)
	@cp -r /tmp/fizzbee-extract/fizzbee-$(FIZZBEE_VERSION)-$(FIZZBEE_OS)_$(FIZZBEE_ARCH)/* $(FIZZ_DIR)/
	@chmod +x $(FIZZ_BIN)
	@rm -rf /tmp/fizzbee.tar.gz /tmp/fizzbee-extract
	@echo "Fizzbee installed to $(FIZZ_DIR)"
	@echo "Run 'make fizzbee' to verify specs"

## fizzbee: Run Fizzbee model checker on all .fizz specs
fizzbee: $(FIZZ_BIN)
	@echo "=== Fizzbee: LeaderlessLog ==="
	@cd fizzbee && $(FIZZ_BIN) LeaderlessLog.fizz 2>&1 | tee /tmp/fizzbee-leaderless.txt
	@grep -q "PASSED" /tmp/fizzbee-leaderless.txt || (echo "FAILED: LeaderlessLog"; exit 1)
	@echo ""
	@echo "=== Fizzbee: TaskClaiming ==="
	@cd fizzbee && $(FIZZ_BIN) TaskClaiming.fizz 2>&1 | tee /tmp/fizzbee-taskclaiming.txt
	@grep -q "PASSED" /tmp/fizzbee-taskclaiming.txt || (echo "FAILED: TaskClaiming"; exit 1)
	@echo ""
	@echo "All Fizzbee specs PASSED"

$(FIZZ_BIN):
	@echo "Fizzbee not found at $(FIZZ_BIN). Run 'make fizzbee-install' first."
	@exit 1

## tlaplus-install: Download TLA+ tools jar
tlaplus-install:
	@echo "Downloading tla2tools.jar $(TLA2TOOLS_VERSION)..."
	@curl -fsSL -o $(TLA2TOOLS_JAR) "$(TLA2TOOLS_URL)"
	@echo "tla2tools.jar installed to $(TLA2TOOLS_JAR)"

$(TLA2TOOLS_JAR):
	@echo "tla2tools.jar not found at $(TLA2TOOLS_JAR). Run 'make tlaplus-install' first."
	@exit 1

## tlaplus: Run TLC model checker on all TLA+ specs
tlaplus: $(TLA2TOOLS_JAR)
	@echo "=== TLA+: LeaderlessLog ==="
	cd tlaplus && java -jar tla2tools.jar -config LeaderlessLog.cfg LeaderlessLog.tla
	@echo ""
	@echo "=== TLA+: TaskClaiming ==="
	cd tlaplus && java -jar tla2tools.jar -config TaskClaiming.cfg TaskClaiming.tla

## verify: Run both Fizzbee and TLA+ model checkers
verify: fizzbee tlaplus

## check: Alias for verify
check: verify

## clean: Remove Fizzbee output artifacts
clean:
	rm -rf fizzbee/out fizzbee/*.json
