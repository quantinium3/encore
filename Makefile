VERSION := $(shell awk -F'"' '/^version/{print $$2; exit}' Cargo.toml)

.PHONY: build test release

build:
	cargo build --release --workspace --bins --locked

test:
	cargo test --workspace --locked

release:
	git tag -a v$(VERSION) -m "v$(VERSION)"
	git push origin v$(VERSION)
