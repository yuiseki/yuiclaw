.PHONY: all install install-deps install-acore install-amem install-abeat install-acomm \
        install-acomm-tui update build build-deps clean submodule-init test

# Default: install all components
all: install

# Install yuiclaw + all dependencies
install: install-deps
	cargo install --path .

# Install all dependency components (order matters: acore → acomm → amem/abeat → acomm-tui)
install-deps: submodule-init install-acore install-amem install-abeat install-acomm install-acomm-tui

# Initialise git submodules if not yet cloned
submodule-init:
	git submodule update --init --recursive

# acore (required by acomm — install first)
install-acore:
	cargo install --path deps/acore

# amem
install-amem:
	cargo install --path deps/amem

# abeat
install-abeat:
	cargo install --path deps/abeat

# acomm Rust bridge (depends on acore)
install-acomm:
	cargo install --path deps/acomm

# acomm TypeScript TUI — installs npm deps and links acomm-tui into PATH
install-acomm-tui:
	cd deps/acomm/tui && npm install --legacy-peer-deps
	cd deps/acomm/tui && npm link

# Local build only (outputs to target/, does not install)
build: build-deps
	cargo build --release

build-deps: submodule-init
	cargo build --release --manifest-path deps/acore/Cargo.toml
	cargo build --release --manifest-path deps/amem/Cargo.toml
	cargo build --release --manifest-path deps/abeat/Cargo.toml
	cargo build --release --manifest-path deps/acomm/Cargo.toml

# Update all submodules to their latest remote state
update:
	git submodule update --remote --merge

# Remove local build artifacts
clean:
	cargo clean

# Run all tests (Rust + TypeScript TUI)
test:
	cargo test
	cd deps/acomm/tui && npx vitest run
