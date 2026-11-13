# Privacy

The distributable project excludes developer usernames/home paths, local handoff
notes, conversations, provider configuration, generated logs, state and build
artifacts. The GitHub account in repository/install URLs is intentional public
project attribution. Commit identity uses GitHub's no-reply address.

Release builds use path remapping; the executable does not embed a developer's
source-root constant. Optional development source protection uses the explicit
runtime `--source-root` argument instead.

## Data created when you use Arc

Installation and upgrades keep private config/Arc-file snapshots under the
selected config directory's `arc-mcp/backups/`. These may include credentials
already present in your OpenCode config. Unix backup directories/files use
owner-only modes; Windows inherits the parent directory's ACLs. Backups and the
installation receipt/lock remain local and are not included in release bundles.
Identical reruns create no additional backup. Backups have no automatic expiry.

Arc stores workflow goals, acceptance criteria, project paths, command strings,
bounded output, timestamps, scoped source hashes, review findings and native
session identifiers in a local state directory. On macOS this defaults to
`~/Library/Application Support/arc-mcp`; `--data-home` or `ARC_DATA_HOME` can select another external
directory. Unix files use owner-only mode bits; Windows storage inherits the
parent directory's ACLs. Use a private user-owned data location. This is not
encryption. Same-user processes
can read or modify them. There is no automatic retention/garbage-collection policy.

Command output is not automatically sanitized. Source fingerprint exclusions do
not prevent a command from printing a secret. Avoid placing credentials in task
text, command lines or captured output. Review project-specific retention needs
before sharing any runtime artifacts.

The native plugin sends evidence packets through OpenCode's selected model
provider. Its privacy/retention policies apply. Source read by reviewers may also
enter those model requests. No separate analytics, remote logging or telemetry
endpoint is implemented by Arc. Installing downloads code/dependencies from
GitHub and npm, plus nodejs.org when a managed Node.js runtime is needed.
Building from source also downloads Rust dependencies from crates.io. The optional
dependency audit sends only public package names and versions to OSV.

To remove stored workflow data, first stop active checks/OpenCode, then remove the
chosen Arc state directory. OpenCode maintains its own session history separately;
removing Arc state does not delete those sessions or a provider's retained data.
