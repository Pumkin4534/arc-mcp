# Public release

The owner explicitly approved publication of the MCP repository. The public
website is maintained in `arc-mcp-site`. Keep the release and installation
instructions synchronized when publishing future versions.

Release checks:

- Review the source tree and commit history for identifiers and secrets.
- Run privacy scanning on source and the path-remapped release executable.
- Run Gitleaks, npm audit and the Rust dependency OSV audit; record real findings.
- Run Rust/plugin/installer tests and the isolated OpenCode smoke scenarios.
- Review the website at desktop/mobile sizes and its installation labels.
- Confirm no runtime state, test transcripts, generated logs or binaries are staged.
- Keep commit author/committer email on GitHub's no-reply domain.
- Preserve the owner's publication scope and the existing installation.

The website bootstrap uses the pinned public source archive without requiring
GitHub CLI access. Keep the single website curl URL stable and verify anonymous
download and installation before advertising a new version.
