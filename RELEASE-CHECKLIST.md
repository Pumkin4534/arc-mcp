# Public release — requires owner approval

The repository must remain **private** until the owner explicitly approves.

Before making it public:

- Review the source tree and commit history for identifiers and secrets.
- Run privacy scanning on source and the path-remapped release executable.
- Run Gitleaks, npm audit and the Rust dependency OSV audit; record real findings.
- Run Rust/plugin/installer tests and the isolated OpenCode smoke scenarios.
- Review the website at desktop/mobile sizes and its private/public install labels.
- Confirm no runtime state, test transcripts, generated logs or binaries are staged.
- Keep commit author/committer email on GitHub's no-reply domain.
- Obtain explicit owner approval before changing repository visibility.

After approval, make the repository public, verify the version-tagged curl URL,
and only then enable GitHub Pages from `site/` (or use the chosen hosting service).
The website currently labels the anonymous curl command as pending public release.
Update that status only when the command is actually accessible anonymously.
