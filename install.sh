#!/usr/bin/env bash
set -euo pipefail

BIN_NAME="${BIN_NAME:-cc-switch-modelhub-proxy}"
REPO="${REPO:-SDGLBL/cc-switch}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
TAG="${TAG:-}"
VERSION="${VERSION:-}"

usage() {
  cat <<EOF
Install cc-switch-modelhub-proxy from GitHub Releases.

Usage:
  install.sh [VERSION_OR_TAG]

Environment:
  REPO         GitHub repository, default: $REPO
  TAG          Exact release tag, for example modelhub-proxy-v0.1.0
  VERSION      Release version, for example 0.1.0 or v0.1.0
  INSTALL_DIR  Install directory, default: $INSTALL_DIR
  GH_TOKEN     Optional GitHub token for private repositories or rate limits

Examples:
  curl -fsSL https://raw.githubusercontent.com/SDGLBL/cc-switch/main/install.sh | bash
  TAG=modelhub-proxy-v0.1.0 ./install.sh
  INSTALL_DIR=/usr/local/bin ./install.sh modelhub-proxy-v0.1.0
EOF
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  exit 0
fi

if [ -n "${1:-}" ]; then
  if [ -n "$TAG" ] || [ -n "$VERSION" ]; then
    echo "Pass the version/tag either as an argument or via TAG/VERSION, not both." >&2
    exit 1
  fi
  VERSION="$1"
fi

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

detect_platform() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os_part="darwin" ;;
    Linux) os_part="linux" ;;
    *)
      echo "Unsupported OS: $os" >&2
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64)
      if [ "$os_part" = "darwin" ]; then
        echo "macOS x86_64 is not published for this ModelHub proxy release." >&2
        echo "Use an Apple Silicon Mac, Linux build, or build cc-switch-modelhub-proxy from source." >&2
        exit 1
      fi
      arch_part="x86_64"
      ;;
    arm64|aarch64) arch_part="aarch64" ;;
    *)
      echo "Unsupported architecture: $arch" >&2
      exit 1
      ;;
  esac

  printf '%s-%s\n' "$os_part" "$arch_part"
}

normalize_version_tag() {
  value="$1"
  case "$value" in
    modelhub-proxy-v*) printf '%s\n' "$value" ;;
    v*) printf 'modelhub-proxy-%s\n' "$value" ;;
    *) printf 'modelhub-proxy-v%s\n' "$value" ;;
  esac
}

github_api() {
  path="$1"
  if [ -n "${GH_TOKEN:-}" ]; then
    curl -fsSL \
      -H "Accept: application/vnd.github+json" \
      -H "Authorization: Bearer $GH_TOKEN" \
      -H "X-GitHub-Api-Version: 2022-11-28" \
      "https://api.github.com/repos/$REPO$path"
  else
    curl -fsSL \
      -H "Accept: application/vnd.github+json" \
      -H "X-GitHub-Api-Version: 2022-11-28" \
      "https://api.github.com/repos/$REPO$path"
  fi
}

find_latest_tag() {
  if command -v gh >/dev/null 2>&1; then
    latest="$(
      gh release list \
        --repo "$REPO" \
        --limit 100 \
        --json tagName \
        --jq '.[] | select(.tagName | startswith("modelhub-proxy-v")) | .tagName' \
        | sed -n '1p'
    )"
    if [ -n "$latest" ]; then
      printf '%s\n' "$latest"
      return
    fi
  fi

  github_api '/releases?per_page=100' \
    | sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\(modelhub-proxy-v[^"]*\)".*/\1/p' \
    | sed -n '1p'
}

download_asset() {
  tag="$1"
  asset="$2"
  output="$3"

  if command -v gh >/dev/null 2>&1; then
    if gh release download "$tag" --repo "$REPO" --pattern "$asset" --dir "$(dirname "$output")" --clobber >/dev/null 2>&1; then
      if [ -f "$(dirname "$output")/$asset" ]; then
        return
      fi
    fi
  fi

  url="https://github.com/$REPO/releases/download/$tag/$asset"
  if command -v curl >/dev/null 2>&1; then
    if [ -n "${GH_TOKEN:-}" ]; then
      curl -fL \
        -H "Authorization: Bearer $GH_TOKEN" \
        -o "$output" \
        "$url"
    else
      curl -fL -o "$output" "$url"
    fi
  elif command -v wget >/dev/null 2>&1; then
    if [ -n "${GH_TOKEN:-}" ]; then
      wget --header="Authorization: Bearer $GH_TOKEN" -O "$output" "$url"
    else
      wget -O "$output" "$url"
    fi
  else
    echo "Missing required command: curl or wget" >&2
    exit 1
  fi
}

hash_file() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    shasum -a 256 "$file" | awk '{print $1}'
  fi
}

cleanup_tmp() {
  if [ -n "${tmpdir:-}" ] && [ -d "$tmpdir" ]; then
    if command -v trash >/dev/null 2>&1; then
      trash "$tmpdir" >/dev/null 2>&1 || true
    else
      echo "Temporary files left at: $tmpdir" >&2
    fi
  fi
}

need_cmd uname
need_cmd tar
need_cmd awk
need_cmd sed

platform="$(detect_platform)"
if [ -n "$TAG" ]; then
  release_tag="$TAG"
elif [ -n "$VERSION" ]; then
  release_tag="$(normalize_version_tag "$VERSION")"
else
  need_cmd curl
  release_tag="$(find_latest_tag)"
fi

if [ -z "$release_tag" ]; then
  echo "Could not find a modelhub-proxy-v* release in $REPO." >&2
  exit 1
fi

case "$release_tag" in
  modelhub-proxy-v*) ;;
  *)
    echo "Release tag must start with modelhub-proxy-v, got: $release_tag" >&2
    exit 1
    ;;
esac

asset="$BIN_NAME-$release_tag-$platform.tar.gz"
tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/cc-switch-modelhub-proxy.XXXXXX")"
trap cleanup_tmp EXIT

echo "Repository: $REPO"
echo "Release:    $release_tag"
echo "Platform:   $platform"
echo "Asset:      $asset"
echo "Install:    $INSTALL_DIR/$BIN_NAME"

archive="$tmpdir/$asset"
download_asset "$release_tag" "$asset" "$archive"

checksum_file="$tmpdir/$asset.sha256"
if download_asset "$release_tag" "$asset.sha256" "$checksum_file"; then
  expected="$(awk '{print $1}' "$checksum_file" | sed -n '1p')"
  actual="$(hash_file "$archive")"
  if [ "$expected" != "$actual" ]; then
    echo "Checksum mismatch for $asset" >&2
    echo "expected: $expected" >&2
    echo "actual:   $actual" >&2
    exit 1
  fi
  echo "Checksum:  ok"
else
  echo "Checksum:  unavailable, continuing without verification" >&2
fi

extract_dir="$tmpdir/extract"
mkdir -p "$extract_dir"
tar -xzf "$archive" -C "$extract_dir"

binary="$extract_dir/$BIN_NAME"
if [ ! -f "$binary" ]; then
  binary="$(find "$extract_dir" -type f -name "$BIN_NAME" | sed -n '1p')"
fi
if [ -z "$binary" ] || [ ! -f "$binary" ]; then
  echo "Downloaded archive does not contain $BIN_NAME" >&2
  exit 1
fi

mkdir -p "$INSTALL_DIR"
install -m 0755 "$binary" "$INSTALL_DIR/$BIN_NAME"

echo
echo "Installed $BIN_NAME to $INSTALL_DIR/$BIN_NAME"
if ! command -v "$BIN_NAME" >/dev/null 2>&1; then
  echo "Add $INSTALL_DIR to PATH to run $BIN_NAME directly." >&2
fi
echo
echo "Try:"
echo "  $INSTALL_DIR/$BIN_NAME --help"
