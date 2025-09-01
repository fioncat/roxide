#!/bin/bash

set -eu

if [[ -f "secrets.sh" ]]; then
	source "secrets.sh"
fi

TEST_GITHUB_TOKEN=${TEST_GITHUB_TOKEN:-""}

TEST_GIT_REPO_PATH="./tests/roxide_git"
if [[ ! -d "$TEST_GIT_REPO_PATH" ]]; then
	git clone https://github.com/fioncat/roxide.git "$TEST_GIT_REPO_PATH"
fi

TEST_GIT="true" cargo test
