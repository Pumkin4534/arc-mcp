# Public release — requires owner approval

The repository must remain **private** until the owner explicitly approves.
The website has separately been approved and published in `arc-mcp-site`;
that approval does not authorize making the MCP source public.

Before making it public:

- Review the source tree and commit history for identifiers and secrets.
- Run privacy scanning on source and the path-remapped release executable.
- Run Gitleaks, npm audit and the Rust dependency OSV audit; record real findings.
- Run Rust/plugin/installer tests and the isolated OpenCode smoke scenarios.
- Review the website at desktop/mobile sizes and its private/public install labels.
- Confirm no runtime state, test transcripts, generated logs or binaries are staged.
- Keep commit author/committer email on GitHub's no-reply domain.
- Obtain explicit owner approval before changing repository visibility.

After source-release approval, make the MCP repository public and update the
separate website bootstrap to fetch the versioned public source without requiring
GitHub CLI access. Keep the single website curl URL stable, verify anonymous
installation, and then update the private-preview access labels.
