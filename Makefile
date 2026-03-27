# ==============================================================================
# Makefile for leasehund
# ==============================================================================
#
# This Makefile provides convenient commands for development, testing, and
# publishing the leasehund crate. It uses the same commands as the GitHub
# Actions CI/CD workflows to ensure consistency between local development
# and continuous integration.
#
# Usage:
#   make help          - Show all available targets
#   make ci            - Run all CI checks (fmt-check, clippy, test)
#   make pre-commit    - Run pre-commit checks (fmt, clippy, test)
#   make publish       - Publish crate to crates.io
#
# ==============================================================================

.PHONY: help
help: ## Show this help message
	@echo "Available targets:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

# Rust version - keep in sync with .github/workflows/ci.yml
RUST_VERSION := 1.88

.PHONY: install-rust
install-rust: ## Install Rust toolchain with required components
	rustup toolchain install $(RUST_VERSION)
	rustup component add rustfmt clippy --toolchain $(RUST_VERSION)

.PHONY: check
check: ## Run cargo check
	cargo +$(RUST_VERSION) check --all-features

.PHONY: fmt
fmt: ## Format all code
	cargo +$(RUST_VERSION) fmt --all

.PHONY: fmt-check
fmt-check: ## Check code formatting
	cargo +$(RUST_VERSION) fmt --all --check

.PHONY: clippy
clippy: ## Run clippy lints
	cargo +$(RUST_VERSION) clippy --all-features -- -D warnings

.PHONY: test
test: ## Run all tests
	cargo +$(RUST_VERSION) test --all-features

.PHONY: test-doc
test-doc: ## Run documentation tests
	cargo +$(RUST_VERSION) test --doc --all-features

.PHONY: build
build: ## Build the crate
	cargo +$(RUST_VERSION) build --all-features

.PHONY: build-release
build-release: ## Build the crate in release mode
	cargo +$(RUST_VERSION) build --all-features --release

.PHONY: doc
doc: ## Generate documentation
	cargo +$(RUST_VERSION) doc --all-features --no-deps

.PHONY: doc-open
doc-open: ## Generate and open documentation
	cargo +$(RUST_VERSION) doc --all-features --no-deps --open

.PHONY: clean
clean: ## Clean build artifacts
	cargo clean

.PHONY: ci
ci: fmt-check clippy test test-doc ## Run all CI checks

.PHONY: pre-commit
pre-commit: fmt clippy test ## Run pre-commit checks

.PHONY: verify-version
verify-version: ## Verify version consistency between Cargo.toml and git tag
	@echo "Checking version..."
	@VERSION=$$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'); \
	echo "  Cargo.toml version: $$VERSION"; \
	TAG=$$(git describe --tags --abbrev=0 2>/dev/null || echo "no tags"); \
	echo "  Latest git tag:     $$TAG"; \
	if [ "$$TAG" != "no tags" ]; then \
		TAG_VERSION=$${TAG#v}; \
		if [ "$$VERSION" != "$$TAG_VERSION" ]; then \
			echo "  ⚠ Version mismatch: Cargo.toml=$$VERSION, tag=$$TAG_VERSION"; \
		else \
			echo "  ✓ Versions match"; \
		fi \
	fi

.PHONY: publish-dry-run
publish-dry-run: ## Dry run of publishing to crates.io
	cargo +$(RUST_VERSION) publish --dry-run --all-features

.PHONY: publish
publish: ## Publish crate to crates.io (requires CARGO_REGISTRY_TOKEN)
	cargo +$(RUST_VERSION) publish --all-features

.PHONY: update-deps
update-deps: ## Update dependencies
	cargo update

.PHONY: all
all: ci doc ## Run all checks and build documentation
