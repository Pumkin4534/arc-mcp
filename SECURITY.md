# Security model

Arc is a workflow aid, not a tamper-proof execution or review attestation system.
Native mode reports `native-advisory` and `tamper_protected:false`. The local
`native-host` adapter is unauthenticated same-user IPC. Its operations are absent
from the public MCP catalog, but ordinary native shell tools can still invoke it.

MCP-owned checks receive macOS Seatbelt restrictions. Unknown pre-existing
hardlinks to protected files refuse startup; identity inventory is cached until
restart. Native OpenCode tools, unrelated MCPs and concurrent same-user processes
are outside that boundary. Process-group termination cannot guarantee cleanup of
deliberately detached descendants. Before/after fingerprints cannot detect every
concurrent change-and-restore. See the README for exclusions and supported scope.

The installer uses versioned HTTPS source, locked dependencies, local compilation
with path remapping, conflict checks and a configuration backup. It does not claim
cryptographic release attestation. Review `install.sh` before execution if your
environment requires manual review of downloaded code.

Please report issues through the repository's GitHub security advisory interface
when available. During private preview, contact the repository owner privately.
Do not post credentials, local evidence archives or personal paths in public issues.

This MCP repository has no public Pages deployment and remains private. The
website is published separately in `arc-mcp-site` with owner authorization.
Public release of the MCP source is a separate explicit owner decision. See
`RELEASE-CHECKLIST.md`.
