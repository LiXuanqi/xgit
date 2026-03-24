#!/usr/bin/env sh

set -eu

REPO="LiXuanqi/xgit"
BINARY_NAME="xg"

log() {
  printf '%s\n' "$*"
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

detect_arch() {
  arch="$(uname -m)"
  case "$arch" in
    x86_64|amd64)
      printf '%s\n' "x86_64"
      ;;
    aarch64|arm64)
      printf '%s\n' "aarch64"
      ;;
    *)
      fail "unsupported architecture: $arch"
      ;;
  esac
}

detect_target() {
  os="$(uname -s)"
  [ "$os" = "Linux" ] || fail "this installer currently supports Linux only"
  printf '%s-unknown-linux-musl\n' "$(detect_arch)"
}

resolve_version() {
  if [ -n "${XG_VERSION:-}" ]; then
    printf '%s\n' "$XG_VERSION"
    return
  fi

  curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" |
    sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' |
    head -n 1
}

usage() {
  cat <<'EOF'
Usage: install.sh [--dir <install_dir>] [--version <tag>]

Options:
  --dir <install_dir>   Install xg into this directory
  --version <tag>       Install a specific release tag, for example v0.2.7
  -h, --help            Show this help message
EOF
}

parse_args() {
  install_dir="${XG_INSTALL_DIR:-$HOME/.local/bin}"
  requested_version="${XG_VERSION:-}"

  while [ "$#" -gt 0 ]; do
    case "$1" in
      --dir)
        [ "$#" -ge 2 ] || fail "--dir requires a value"
        install_dir="$2"
        shift 2
        ;;
      --version)
        [ "$#" -ge 2 ] || fail "--version requires a value"
        requested_version="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        fail "unknown argument: $1"
        ;;
    esac
  done
}

main() {
  need_cmd curl
  need_cmd tar

  parse_args "$@"
  version="$(resolve_version)"
  if [ -n "$requested_version" ]; then
    version="$requested_version"
  fi
  [ -n "$version" ] || fail "failed to resolve the latest release version"

  target="$(detect_target)"
  asset="${BINARY_NAME}-${version}-${target}.tar.gz"
  url="https://github.com/${REPO}/releases/download/${version}/${asset}"

  tmp_dir="$(mktemp -d)"
  archive_path="${tmp_dir}/${asset}"

  trap 'rm -rf "$tmp_dir"' EXIT INT TERM

  log "Installing ${BINARY_NAME} ${version} for ${target}"
  mkdir -p "$install_dir"
  curl -fsSL "$url" -o "$archive_path"
  tar -xzf "$archive_path" -C "$tmp_dir"
  install "${tmp_dir}/${BINARY_NAME}-${version}-${target}/${BINARY_NAME}" "${install_dir}/${BINARY_NAME}"

  log "Installed to ${install_dir}/${BINARY_NAME}"
  case ":$PATH:" in
    *":${install_dir}:"*)
      ;;
    *)
      log "Add ${install_dir} to your PATH if it is not already there."
      ;;
  esac
}

main "$@"
