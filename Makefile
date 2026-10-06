.PHONY: build test lint cli-help deploy-devnet deploy-mainnet

build:
	anchor build --arch v2

test:
	anchor build --arch v2
	cargo test --workspace

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

cli-help:
	cargo run -q -p trace-cli -- help

deploy-devnet:
	anchor build --arch v2
	anchor deploy --provider.cluster devnet

deploy-mainnet:
	anchor build --arch v2 --verifiable
	anchor deploy --provider.cluster mainnet
