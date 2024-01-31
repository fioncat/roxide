#!/bin/bash

set -eu

targets=( \
	"x86_64-unknown-linux-gnu" \
	"aarch64-unknown-linux-gnu" \
	"x86_64-apple-darwin" \
	"aarch64-apple-darwin" \
)

BOLD="$(tput bold 2>/dev/null || printf '')"
GREY="$(tput setaf 0 2>/dev/null || printf '')"
UNDERLINE="$(tput smul 2>/dev/null || printf '')"
RED="$(tput setaf 1 2>/dev/null || printf '')"
GREEN="$(tput setaf 2 2>/dev/null || printf '')"
YELLOW="$(tput setaf 3 2>/dev/null || printf '')"
BLUE="$(tput setaf 4 2>/dev/null || printf '')"
MAGENTA="$(tput setaf 5 2>/dev/null || printf '')"
CYAN="$(tput setaf 6 2>/dev/null || printf '')"
RESET="$(tput sgr0 2>/dev/null || printf '')"

info() {
	printf '%s\n' "${BOLD}${GREY}>${RESET} ${CYAN}$*${RESET}"
}

error() {
	printf '%s\n' "${RED}x $*${RESET}" >&2
}

shell_join() {
	local arg
	printf "%s" "$1"
	shift
	for arg in "$@"; do
		printf " "
		printf "%s" "${arg// /\ }"
	done
}

confirm() {
	read -p "$1 (y/n) " -n 1 -r
	echo
	if [[ $REPLY =~ ^[Yy]$ ]]; then
		return 0
	fi
	error "user aborted"
	exit 1
}

execute() {
	shell_exec=$(shell_join "$@")
	if ! "$@"; then
		error "failed to execute command"
		exit 1
	fi
}

has() {
	command -v "$1" 1>/dev/null 2>&1
}

download() {
	file="$1"
	url="$2"

	if has wget; then
		execute "wget" "-q" "--output-document=$file" "$url"
	elif has curl; then
		execute "curl" "--fail" "--location" "--output" "$file" "$url"
	elif has fetch; then
		execute "fetch" "--output=$file" "$url"
	else
		error "No HTTP download program (curl, wget, fetch) found, exiting…"
		return 1
	fi
}

# Test if a location is writeable by trying to write to it.
test_writeable() {
	path="${1:-}/test.txt"
	if touch "${path}" 2>/dev/null; then
		rm "${path}"
		return 0
	else
		return 1
	fi
}

# Currently supporting:
#   - x86_64
#   - aarch64
detect_arch() {
	arch="$(uname -m | tr '[:upper:]' '[:lower:]')"
	case "${arch}" in
		amd64|x86_64) arch="x86_64" ;;
		arm64) arch="aarch64" ;;
	esac
	printf '%s' "${arch}"
}

detect_os() {
	os="$(uname -s | tr '[:upper:]' '[:lower:]')"
	case "${os}" in
		linux) os="unknown-linux-gnu" ;;
		darwin) os="apple-darwin" ;;
	esac
	printf '%s' "${os}"
}

ensure_command() {
	if has $1; then
		return 0
	fi
	error "command $1 is required to install roxide"
}

ensure_command "perl"
ensure_command "tar"

if [[ $# -ge 1 ]]; then
	BIN_DIR="$1"
else
	BIN_DIR="/usr/local/bin"
fi
TMP_DIR="/tmp/roxide-install"
BASE_URL="https://github.com/fioncat/roxide/releases"

SKIP_CONFIRM="false"
if [[ $# -ge 2 ]]; then
	SKIP_CONFIRM="$2"
fi

PLATFORM="$(detect_os)"
ARCH="$(detect_arch)"

TARGET="${ARCH}-${PLATFORM}"
URL="${BASE_URL}/latest/download/roxide_${TARGET}.tar.gz"

SUPPORT=""
for support_target in "${targets[@]}"; do
	if [[ "${TARGET}" == "${support_target}" ]]; then
		SUPPORT="true"
	fi
done

if [ -z ${SUPPORT} ]; then
	error "Sorry, now we donot support your platform: ${TARGET}"
	exit 1
fi

if [[ ! "${SKIP_CONFIRM}" == "true" ]]; then
	confirm "Install roxide to ${BIN_DIR}?"
else
	info "About to install roxide to ${BIN_DIR}"
fi


if [ -d ${TMP_DIR} ]; then
	rm -r ${TMP_DIR}
fi
mkdir -p ${TMP_DIR}
ARCHIVE_FILE="${TMP_DIR}/roxide.tar.gz"
info "Downloading roxide"
download ${ARCHIVE_FILE} ${URL}

info "Unzipping file"
execute "tar" "-xzf" "${TMP_DIR}/roxide.tar.gz" -C "${TMP_DIR}"

TMP_BIN_PATH="${TMP_DIR}/roxide"
if test_writeable "${BIN_DIR}"; then
	info "Moving binary file"
	execute "mv" "${TMP_BIN_PATH}" "${BIN_DIR}"
else
	info "Escalated permissions are required to install to ${BIN_DIR}"
	execute "sudo" "mv" "${TMP_BIN_PATH}" "${BIN_DIR}"
fi

rm -r ${TMP_DIR}

SHELL_TYPE=$(basename $SHELL)
case "$SHELL_TYPE" in
    "zsh")
		PROFILE_PATH=${HOME}/.zshrc
		;;
	"bash")
		PROFILE_PATH=${HOME}/.bashrc
        ;;
    *)
		error "Sorry, now we donot support your shell ${SHELL_TYPE}"
		exit 1
        ;;
esac

INIT_ROXIDE_SEARCH="source <(roxide init ${SHELL_TYPE})"
INIT_ROXIDE="
if command -v roxide &> /dev/null; then
	source <(roxide init ${SHELL_TYPE})
fi
"

if ! grep -q "$INIT_ROXIDE_SEARCH" "$PROFILE_PATH"; then
	echo ""
	confirm "Do you want to install shell support for roxide to ${PROFILE_PATH}?"
	info "Write init script to ${PROFILE_PATH}"
	echo "$INIT_ROXIDE" >> ${PROFILE_PATH}
fi


if [[ "${SKIP_CONFIRM}" == "true" ]]; then
	exit 0
fi

cat << EOF

Congratulations! roxide has been already installed (or updated) to ${CYAN}${BIN_DIR}${RESET}.
For more details, please refer to: https://github.com/fioncat/roxide
EOF
