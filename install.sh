#!/usr/bin/env bash
# Keep this source entry point on the same dependency-aware installer as the site.
set -euo pipefail
umask 077
work="$(mktemp -d "${TMPDIR:-/tmp}/arc-bootstrap.XXXXXXXX")"
trap 'rm -rf "$work"' EXIT
curl -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
  'https://pumkin4534.github.io/install.sh' --output "$work/install.sh"
/bin/bash "$work/install.sh"
