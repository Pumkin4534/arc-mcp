# Public release

The owner explicitly approved publication of the MCP repository. The canonical
website is maintained in `Pumkin4534/Pumkin4534.github.io`; `arc-mcp-site` preserves
the legacy URL. Keep release versions, installers and website instructions aligned.

Release checks:

- Review the source tree and commit history for identifiers and secrets.
- Run privacy scanning on source and the path-remapped release executable.
- Run Gitleaks, npm audit and the Rust dependency OSV audit; record real findings.
- Require a successful six-platform workflow on the release commit: Windows,
  Ubuntu and macOS, each on x64 and ARM64. Run Rust/Clippy, plugin/installer tests,
  real isolated OpenCode completion and Stop, packaged-runtime installation, and
  bootstrap installation with both existing and freshly downloaded Node.js.
- Exercise an actual previous-release installation followed by the new packaged
  upgrader and an identical rerun. Verify backups, retained settings/history and
  rollback on injected activation failures.
- Review the website at desktop/mobile sizes and its installation labels.
- Confirm no runtime state, test transcripts, generated logs or binaries are staged.
- Keep commit author/committer email on GitHub's no-reply domain.
- Preserve the owner's publication scope and the existing installation.

## Publication order

1. Collect all six bundles and their `.sha256` files from one successful workflow
   on the exact release commit. Verify every archive digest and inspect its contents.
   Do not mix artifacts from different revisions or publish failed-run artifacts.
2. Create the immutable version tag and publish its twelve release assets before
   advancing the public installers. Do not move an existing release tag.
3. Fast-forward core `main` to the verified release commit, then publish the main
   website. Website wrappers delegate to core `main`, so publishing them before
   replacing an older delegating core installer would cause recursion.
4. Verify anonymous release downloads and the live website-to-installer chain in
   an isolated home. Check Windows command detection, Unix command detection,
   legacy URL forwarding and the restart instructions.

The stable entry points are `https://pumkin4534.github.io/install.sh` and
`https://pumkin4534.github.io/install.ps1`. They require no GitHub login or compiler.
Node.js is reused or installed locally from its official checksum-pinned archive.
macOS retains Seatbelt; Windows/Ubuntu explicitly use native checks without a
filesystem/network sandbox. Preserve that distinction in release announcements.
