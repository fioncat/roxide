.PHONY: test-slow
test-slow:
	@bash ./hack/run_full_tests.sh

.PHONY: test-fast
test-fast:
	@cargo test

.PHONY: test-clean
test-clean:
	@rm -rf ./tests/*
	@touch ./tests/.gitkeep

