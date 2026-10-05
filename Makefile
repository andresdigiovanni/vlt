# vlt — development tasks. Run `make` or `make help` to list them.

CARGO ?= cargo

.DEFAULT_GOAL := help
.PHONY: help \
	build build-release run \
	test test-unit test-integration \
	lint check \
	bench bench-models bench-latency \
	ci clean

##@ Help

help: ## Show this help
	@awk 'BEGIN { FS = ":.*## "; printf "Usage: make \033[36m<target>\033[0m\n" } \
		/^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } \
		/^[a-zA-Z_-]+:.*## / { printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

##@ Build

build: ## Debug build
	$(CARGO) build

build-release: ## Release build, locked to Cargo.lock (what CI publishes)
	$(CARGO) build --release --locked

run: ## Run the debug binary; pass arguments with ARGS="search foo"
	$(CARGO) run -- $(ARGS)

##@ Test (deterministic fake models, a few seconds)

test: ## Unit tests plus the fast integration tests
	$(CARGO) test

test-unit: ## Unit tests only (src/)
	$(CARGO) test --lib

test-integration: ## Integration tests only (tests/: CLI, installer, MCP)
	$(CARGO) test --test cli --test install --test mcp_smoke

##@ Lint

lint: ## Clippy over every target (benches included), warnings are errors
	$(CARGO) clippy --all-targets -- -D warnings

check: lint test ## Everything that must pass before merging: lint + test

##@ Benchmarks (need the real models in ~/.vlt/models: run `vlt install` first)

bench: bench-models bench-latency ## Run every benchmark (~17 min)

bench-models: ## Real-model checks (PASS/FAIL per check, non-zero exit on failure)
	$(CARGO) bench --bench real_models

bench-latency: ## PRD 10 latency benchmark on a synthetic 5,000-document vault (~17 min)
	$(CARGO) bench --bench latency

##@ CI and housekeeping

ci: lint test build-release ## What CI-quality means: lint, test and the locked release build

clean: ## Remove build artifacts
	$(CARGO) clean
