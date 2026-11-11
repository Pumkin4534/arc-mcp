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
| Public anonymous website bootstrap → source build → installation | Passed in a fresh home with no GitHub/provider credentials; real release compilation, installed config/plugin, version 0.1.1, MCP initialization and all seven tools verified |
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

The owner has explicitly approved public source release. The repository and
website now use a public, version-pinned installation path without requiring
GitHub CLI credentials. The privacy scan and full-history Gitleaks scan were
refreshed with no findings; npm audit and the OSV check of 116 Rust dependency
versions reported no vulnerabilities. See `RELEASE-CHECKLIST.md` for the process.

The public installer was then exercised end to end against the deployed website
and public version-tagged source archive. The fresh-home environment contained no
GitHub credentials, CLI configuration or provider credentials. Compilation and
installation succeeded; the binary contained none of the checked identifying
build paths. A real stdio handshake returned version 0.1.1 and the seven-tool
catalog. The operator's existing installation was not the installation target.
