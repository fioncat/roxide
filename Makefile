all:
	@cargo build --release --locked --color=always --verbose

.PHONY: install
install:
	@cargo install --path . --force

.PHONY: clean
clean:
	@cargo clean
