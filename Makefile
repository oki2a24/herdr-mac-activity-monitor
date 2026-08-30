.PHONY: fmt fmt-check check lint test audit verify ci

fmt:
	cargo fmt

fmt-check:
	cargo fmt --all -- --check

check:
	cargo check --all-targets --all-features

lint:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --all-targets --all-features

audit:
	cargo audit

verify: fmt-check check lint test

ci: verify audit
