# Release verification

## v0.2.0 cross-platform verification

[Platform workflow 43271511562](https://github.com/Pumkin4534/arc-mcp/actions/runs/43271511562)
passed all six jobs on runtime/installer commit `b1b931f`. Release artifacts must
come from a successful workflow on the final tagged commit, including any later
documentation changes; see [RELEASE-CHECKLIST.md](RELEASE-CHECKLIST.md).

| Native runner image | Architecture | Result |
| --- | --- | --- |
| Ubuntu 22.04 | x64 | Passed |
| Ubuntu 22.04 ARM | ARM64 | Passed |
| Windows Server 2022 | x64 | Passed |
| Windows 11 ARM | ARM64 | Passed |
| macOS 15 Intel | x64 | Passed |
| macOS 15 Apple Silicon | ARM64 | Passed |

Each job ran Rust tests, Clippy, the 11 plugin/installer tests, source and release
binary privacy scans, official OpenCode 1.18.25 completion and native Stop
integration, release packaging, packaged installation and a real MCP check.
macOS exercises Seatbelt and rejects the native executor before launch. Windows
and Ubuntu exercise native output/exit, timeout, cancellation, bounded output and
owned-process cleanup without claiming filesystem/network isolation.

The actual platform bootstraps passed both with existing Node.js and with an
unavailable/unusable runtime. In the latter case they downloaded the official
Node.js 22.23.4 archive, verified its pinned checksum and installed it in a fresh
managed directory. The tests verified that no compiler ran. Windows exercised
Windows PowerShell 5.1, including caller-PATH restoration. Release-bundle transport
was a local fixture of the real built archive and checksum; the Node download,
archive extraction, dependency installation and configuration merge were real.
These pre-release tests do not claim to have downloaded unpublished release URLs.

Fresh checks also found no npm audit findings or OSV matches among 117 locked Rust
dependency versions. Source privacy scanning covered 50 files without findings;
the full-history Gitleaks scan reported no leaks. The private preservation baseline
confirmed all 44 tracked original source/installed files unchanged. The separate
website passed nine site/orbit tests and browser checks of Windows/macOS/Linux
command selection, mocked clipboard copy, reduced motion, mobile overflow and
desktop navigation alignment.

Initial candidate runs failed: Windows PowerShell native-argument quoting, cached
Unix Node lookup after installing a runtime, and a test of the unsupported native
runner on macOS. These were fixed before the successful workflow above; failed or
interrupted runs are not counted as successful release verification.

All native review records remain advisory. Deterministic provider tests establish
integration and cancellation behavior, not real-model judgment quality. Scans are
bounded and process cleanup is scoped; see `SECURITY.md` and `PRIVACY.md`.

## Historical v0.1.1 source-release verification

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

## Dependency-aware bootstrap update

The website installer now recovers Rust outside PATH and prepares isolated
Rust/Cargo and Node.js/npm tools when missing or too old. Eleven bootstrap/site
tests passed, including retained existing tools/profiles and Node checksum failure.
A real isolated-home setup installed Rust through official rustup. Initial tool
startup exceeded the test deadline; direct version checks subsequently passed.
A deliberate continuation reused that installed toolchain, verified the official
Node.js 22.23.4 archive, built and installed Arc, and passed the seven-tool MCP
handshake. The initial timed-out run is not reported as a successful full run.
