# Pre-publication verification

Scope: sanitized **0.1.1** source, portable installer and private-preview website.
The original development tree and installed Arc/OpenCode configuration were kept
separate; a private checksum baseline is used to check preservation. That baseline,
local test logs and session IDs are not distributed.

## Completed checks

| Check | Observed result |
| --- | --- |
| Rust suite | 43 tests passed |
| Clippy (`--all-targets -- -D warnings`) and formatting | Passed |
| Node plugin and installer tests | 11 passed |
| Installer coverage | Fresh setup, JSON/JSONC, comments/trailing commas, backup, idempotence, conflicting Arc, malformed config, two configs, symlink refusal |
| Source and remapped release-binary privacy scan | No identifying home paths or configured private identifiers found |
| Gitleaks 8.30.1, staged distributable source | No secret findings |
| npm audit | No reported vulnerabilities |
| OSV audit of 116 Rust dependency versions | No reported vulnerability matches |
| Bash installer syntax | Passed |
| Authenticated private GitHub download → source build → installation | Passed into an isolated configuration directory; installed binary/plugin/config verified and identifying build paths absent |
| Real OpenCode integration with the installed portable layout | Full `/arc` workflow passed with two distinct native read-only reviewer sessions |
| Real native Stop during both reviewers | Passed; no finalization or automatic restart |
| Website | 360/768/1440 layouts without horizontal overflow, 200% zoom, working demo/reset, private/public command selector, copy success/failure, keyboard skip link, reduced motion, image loading and no console errors |
| Website text contrast spot-check | No below-threshold text/background combinations found by the local computed-style check |

The OpenCode integration tests use the actual installed OpenCode 1.18.25 binary,
real MCP processes and actual plugin/agent hooks in disposable homes/projects.
A deterministic local provider drives the conversation. These tests establish
transport, permission, identity, completion and cancellation behavior, **not** the
quality of a real model's judgments. No paid model calls are used.

The test provider and scratch transcripts live in ignored `target/` directories.
No transcripts, test-home credentials, original configuration or build outputs
are included in the Git repository. Clipboard tests replace the browser clipboard
API with an in-memory fixture; they do not write the operator's clipboard.

## Scope of security claims

These scans are bounded checks, not proof that every vulnerability or private
string is absent. The source retains intentional GitHub project attribution.
No protected-review or universal native-tool sandbox claim is made. Runtime
records can contain future users' private task/output data; see `PRIVACY.md`.

No deployment workflow enables public hosting or changes repository visibility.
The anonymous curl command remains unavailable until the owner approves a public
release. See `RELEASE-CHECKLIST.md` before changing that status.
