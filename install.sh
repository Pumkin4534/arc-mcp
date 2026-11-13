#!/usr/bin/env bash
# Canonical prebuilt release installer: no Cargo or compiler required.
set -euo pipefail
umask 077
version='0.2.1'
case "$(uname -s)" in
  Darwin) platform='macos'; node_os='darwin';;
  Linux) platform='linux'; node_os='linux';;
  *) printf '%s\n' 'Use install.ps1 in PowerShell on Windows. This script supports macOS and Ubuntu.' >&2; exit 1;;
esac
case "$(uname -m)" in x86_64|amd64) arch='x64';; arm64|aarch64) arch='arm64';; *) printf '%s\n' 'Unsupported CPU architecture.' >&2; exit 1;; esac
for tool in curl tar; do command -v "$tool" >/dev/null || { printf 'Required system tool missing: %s\n' "$tool" >&2; exit 1; }; done
if command -v sha256sum >/dev/null; then hash_tool='sha256sum'; elif command -v shasum >/dev/null; then hash_tool='shasum'; else printf '%s\n' 'A SHA-256 utility is required.' >&2; exit 1; fi
work="$(mktemp -d "${TMPDIR:-/tmp}/arc-install.XXXXXXXX")"
trap 'rm -rf "$work"' EXIT
fetch() { curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 "$1" --output "$2"; }
verify() {
  [[ "$1" =~ ^[0-9a-fA-F]{64}$ ]] || { printf '%s\n' 'Invalid checksum record.' >&2; exit 1; }
  if [[ "$hash_tool" == sha256sum ]]; then printf '%s  %s\n' "$1" "$2" | sha256sum -c -;
  else printf '%s  %s\n' "$1" "$2" | shasum -a 256 -c -; fi
}
node_ready() {
  command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1 || return 1
  node -e 'if (Number(process.versions.node.split(".")[0]) < 20) process.exit(1)' >/dev/null 2>&1 && npm --version >/dev/null 2>&1
}
if ! node_ready; then
  tools="${ARC_TOOLS_HOME:-${XDG_DATA_HOME:-$HOME/.local/share}/arc-mcp/tools}"
  case "$tools" in /*) ;; *) printf '%s\n' 'ARC_TOOLS_HOME must be absolute.' >&2; exit 1;; esac
  node_version='22.23.4'
  case "$node_os-$arch" in
    darwin-x64) checksum='efcc2b0db6f94bf53eddb038fe2b370277a042bd29e4f19b8f8e23598795b819';;
    darwin-arm64) checksum='c9e1dbc748b33ed2d14f42445191ecaf14c56259c7a9a26d0042c49ec2b66428';;
    linux-x64) checksum='b8492284c76a54baf58d61668dd0e41ee0fc98adabf9503d30f05d08f55b4406';;
    linux-arm64) checksum='f7a1e486dbc8b286847815b477d4fb5e9fb963ed4489c3e1588853d685f589f9';;
  esac
  node_dir="$tools/node-v$node_version-$node_os-$arch"
  export PATH="$node_dir/bin:$PATH"
  if ! node_ready; then
    [[ ! -e "$node_dir" && ! -L "$node_dir" ]] || { printf 'Inspect the incomplete managed Node directory: %s\n' "$node_dir" >&2; exit 1; }
    printf '%s\n' 'Preparing Node.js for installation…'
    fetch "https://nodejs.org/dist/v$node_version/node-v$node_version-$node_os-$arch.tar.gz" "$work/node.tar.gz"
    verify "$checksum" "$work/node.tar.gz"
    mkdir "$work/node"; tar -xzf "$work/node.tar.gz" -C "$work/node" --strip-components=1
    mkdir -p "$tools"; mv "$work/node" "$node_dir"
  fi
  # Readiness checks may have cached an older executable before the managed
  # directory existed. Resolve Node/npm again after the new runtime is in place.
  hash -r
fi
node_ready || { printf '%s\n' 'Node.js/npm setup failed.' >&2; exit 1; }
asset="arc-mcp-$version-$platform-$arch.tar.gz"
release="https://github.com/Pumkin4534/arc-mcp/releases/download/v$version"
printf 'Downloading Arc MCP %s for %s/%s…\n' "$version" "$platform" "$arch"
fetch "$release/$asset.sha256" "$work/checksum"
read -r expected filename < "$work/checksum"
[[ "$filename" == "$asset" ]] || { printf '%s\n' 'Checksum filename mismatch.' >&2; exit 1; }
fetch "$release/$asset" "$work/package.tar.gz"
verify "$expected" "$work/package.tar.gz"
mkdir "$work/package"; tar -xzf "$work/package.tar.gz" -C "$work/package" --strip-components=1
(
  cd "$work/package"
  npm ci --ignore-scripts --no-audit --no-fund
  node scripts/install.mjs --install
)
