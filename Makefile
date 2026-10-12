.PHONY: build test lint

build:
	anchor build --arch v2

test:
	anchor build --arch v2
	cargo test -p trace-registry

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
