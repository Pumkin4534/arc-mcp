# Security model

Arc is a workflow aid, not a tamper-proof execution or review attestation system.
Native mode reports `native-advisory` and `tamper_protected:false`. The local
`native-host` adapter is unauthenticated same-user IPC. Its operations are absent
from the public MCP catalog, but ordinary native shell tools can still invoke it.

macOS MCP-owned checks receive Seatbelt restrictions. Unknown pre-existing
hardlinks to protected files refuse startup; identity inventory is cached until
restart. Native OpenCode tools, unrelated MCPs and concurrent same-user processes
are outside that boundary. Process-group termination cannot guarantee cleanup of
deliberately detached descendants. Before/after fingerprints cannot detect every
concurrent change-and-restore. On Windows and Ubuntu the installed native mode
captures real commands without filesystem/network isolation. Windows launches a
trusted helper, assigns it to a kill-on-close Job Object, and only then sends the
command. This controls owned process cleanup, not service/desktop delegation or
same-user tampering. Unix cleanup is process-group scoped. See the README for
exclusions and supported scope.

The installer uses versioned HTTPS release bundles, SHA-256 verification, locked
installer dependencies, conflict checks and a configuration backup. Release
binaries are built with source/home path remapping. It does not claim
cryptographic release attestation. Review `install.sh` before execution if your
environment requires manual review of downloaded code.

Please report issues through the repository's GitHub security advisory interface
when available, or contact the repository owner privately.
Do not post credentials, local evidence archives or personal paths in public issues.

The MCP source is public with owner authorization. The website is maintained
separately in `arc-mcp-site`; its installer downloads a pinned public source
archive without authentication. See `RELEASE-CHECKLIST.md` for release checks.
