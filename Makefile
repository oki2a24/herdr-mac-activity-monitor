.PHONY: fmt fmt-check check lint test audit version-check build verify ci

version-check:
	@cargo_v=$$(grep '^version' Cargo.toml); \
	plugin_v=$$(grep '^version' herdr-plugin.toml); \
	if [ "$$cargo_v" = "$$plugin_v" ]; then \
		echo "version-check: ($$cargo_v)"; \
	else \
		echo "version-check: MISMATCH" 1>&2; \
		echo "  Cargo.toml:        $$cargo_v" 1>&2; \
		echo "  herdr-plugin.toml: $$plugin_v" 1>&2; \
		exit 1; \
	fi

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

# 配布用バイナリを再ビルドする。
# Cargo.toml の [profile.release] strip = "symbols" により release ビルド自体が strip 済みなので、
# 別途 strip 工程は不要。[[build]] を使わず bin/ へ直接配置する本プラグインの慣習に合わせた。
build:
	cargo build --release
	cp target/release/herdr-activity-monitor bin/herdr-activity-monitor

verify: version-check fmt-check check lint test

ci: verify audit
