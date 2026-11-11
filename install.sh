#!/usr/bin/env bash
set -euo pipefail
umask 077

repository="${ARC_REPOSITORY:-Pumkin4534/arc-mcp}"
ref="${ARC_REF:-v0.1.1}"
if [[ "$(uname -s)" != Darwin ]]; then
  printf '%s\n' 'Arc MCP currently supports macOS only; no changes made.' >&2
  exit 1
fi
if [[ ! "$repository" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || [[ ! "$ref" =~ ^[A-Za-z0-9_.-]+$ ]]; then
  printf '%s\n' 'Invalid repository/ref.' >&2; exit 1
fi
for tool in cargo rustc node npm curl tar; do
  command -v "$tool" >/dev/null || { printf 'Required tool missing: %s\n' "$tool" >&2; exit 1; }
done
node -e 'if (Number(process.versions.node.split(".")[0]) < 20) process.exit(1)' || { printf '%s\n' 'Node.js 20+ is required.' >&2; exit 1; }
xcode-select -p >/dev/null 2>&1 || { printf '%s\n' 'Install Xcode command line tools first: xcode-select --install' >&2; exit 1; }
work="$(mktemp -d "${TMPDIR:-/tmp}/arc-install.XXXXXXXX")"
trap 'rm -rf "$work"' EXIT
if [[ "${ARC_PRIVATE:-0}" == 1 ]]; then
  command -v gh >/dev/null || { printf '%s\n' 'Private installation requires an authenticated GitHub CLI.' >&2; exit 1; }
  gh api "repos/$repository/tarball/$ref" > "$work/source.tar.gz"
else
  curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
    "https://api.github.com/repos/$repository/tarball/$ref" --output "$work/source.tar.gz"
fi
mkdir "$work/source"
tar -xzf "$work/source.tar.gz" -C "$work/source" --strip-components=1
(
  cd "$work/source"
  npm ci --ignore-scripts --no-audit --no-fund
  node scripts/build.mjs
  node scripts/install.mjs --install
)
