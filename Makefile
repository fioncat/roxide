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


.PHONY: test-stable
test-stable:
	@bash -c "for i in {1..120}; do cargo test; sleep 1; done"
