# Arc MCP

**A smaller plan. A stronger finish.** Evidence-led workflows inside OpenCode.

Arc adds 1–5 ordered outcomes, actual check records, two fresh read-only native
reviewers and an advisory completion gate. OpenCode keeps its models, UI and tools.

## Open source · v0.1.1

Arc MCP is publicly available under the MIT license. Explore the workflow and
installation guide at https://pumkin4534.github.io/arc-mcp-site/.

## Install

Requirements: **macOS**, OpenCode (tested with **1.18.25**) and Xcode command line
tools. Missing Rust/Cargo and Node.js/npm are prepared automatically. Compatible
existing tools are reused. Intel macOS is tested; Apple Silicon
builds from source but has not been exercised on the development host. Linux and
Windows check execution are not supported by this release.

### One-line install

```sh
curl -fsSL https://pumkin4534.github.io/arc-mcp-site/install.sh | bash
```

No GitHub login, CLI, or repository invitation is required. The installer
downloads the public versioned source, builds locally with builder paths
remapped, and merges an Arc entry into existing OpenCode JSON/JSONC configuration.
It supports comments and trailing commas, backs up changed configuration and
refuses conflicting Arc installations. It does not replace existing providers or
permissions. Source archives contain no developer sessions, logs or credentials.

Automatically prepared build tools live under
`${XDG_DATA_HOME:-$HOME/.local/share}/arc-mcp/tools` (or `ARC_TOOLS_HOME`). Rust is
installed through official rustup; Node.js uses a checksum-verified official
archive. These build-tool PATH changes are local to the installer process, with
no replacement of existing toolchains or shell-profile edits. If Apple's developer
tools are missing, run `xcode-select --install` first and rerun the same command.

Quit and restart OpenCode after installation:

```text
/arc Implement the change with these acceptance criteria: …
/arc-status
/arc-continue
```

Installed binary: `~/.config/opencode/arc-mcp/bin/arc-mcp` (respects
`XDG_CONFIG_HOME`). The installer also adds the Arc plugin, read-only agent,
workflow skill and three commands. Tests use `ARC_CONFIG_HOME` to select an
isolated configuration directory; normal use requires no environment overrides.

## How it works

1. **Plan:** 1–5 broad dependency-ordered outcomes, retaining all acceptance details.
2. **Build:** use native OpenCode tools and your selected model.
3. **Check:** run real commands and bind bounded output/exit to scoped source
   fingerprints. Failed, unknown, interrupted or stale evidence blocks readiness.
4. **Review:** two fresh native sessions receive the evidence packet and read-only
   project access, not the writer transcript. They run concurrently.
5. **Finish:** current checks and both review judgments must agree. Progress-only
   updates cannot erase a failed judgment. Prior findings follow later packets.

Stop, new input, questions and permission requests interrupt reviews. Arc does not
automatically resume stopped work or replay commands. One active ledger is stored
per project; unfinished work cannot be silently discarded by a replacement plan.

## Privacy and assurance

See [PRIVACY.md](PRIVACY.md) and [SECURITY.md](SECURITY.md).

- No built-in analytics or separate telemetry service.
- During use, local records include task text, paths, commands, output and reviewer
  IDs. Output is **not automatically redacted**. Keep secrets out of commands and
  captured output. State is not included in source packages.
- Reviewer packets use your selected OpenCode model provider. A remote provider
  receives that task/evidence context; this is not an offline-only tool.
- Completion is explicitly `native-advisory`, `tamper_protected:false`. Ordinary
  native tools and same-user processes can modify host records.
- Only MCP-owned checks use macOS Seatbelt. Source observations are scoped and
  non-atomic; ignored/generated/secret content and symlink referents are excluded.
- Cleanup owns a process group, not deliberately detached descendants. Run
  foreground tests, not daemons. Guarded checks deny network/Mach delegation and
  writes outside project/scratch (except `/dev/null`). Install dependencies first.

## Development

```sh
npm ci --ignore-scripts
cargo test
cargo clippy --all-targets -- -D warnings
npm run build
npm test
npm run privacy -- --binary
node scripts/dependency-audit.mjs
npm audit --omit=dev
```

`node tests/opencode-smoke.mjs` uses installed OpenCode with an isolated home and a
local deterministic provider. It verifies integration, not live model judgment
quality. Set `ARC_TEST_SCENARIO=stop` for native cancellation testing.

The current website is maintained separately in `Pumkin4534/arc-mcp-site`.
The `site/` directory here is the earlier private-preview design. No GitHub Pages
deployment is enabled for this MCP repository.

To remove Arc, delete only `mcp.arc` from the selected config and the installed
Arc files: `arc-mcp/`, `plugins/arc-opencode.js`, `agents/arc-reviewer.md`,
`skills/arc-workflow/`, and `commands/{arc,arc-status,arc-continue}.md`. Then restart
OpenCode. Keep your evidence directory if you want to retain workflow history.

MIT licensed. See [LICENSE](LICENSE).
