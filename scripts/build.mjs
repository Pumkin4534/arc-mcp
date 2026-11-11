import { execFileSync } from 'node:child_process';
import { homedir, tmpdir } from 'node:os';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const flags = [
  ...(process.env.CARGO_ENCODED_RUSTFLAGS?.split('\x1f') ?? []),
  `--remap-path-prefix=${homedir()}=/build/home`,
  `--remap-path-prefix=${tmpdir()}=/build/tmp`,
  `--remap-path-prefix=${process.env.CARGO_HOME ?? join(homedir(), '.cargo')}=/build/cargo-home`,
  `--remap-path-prefix=${process.env.RUSTUP_HOME ?? join(homedir(), '.rustup')}=/build/toolchain`,
  `--remap-path-prefix=${root}=/build/arc-mcp`,
];
execFileSync('cargo', ['build', '--release', '--locked'], {
  cwd: root, stdio: 'inherit', env: { ...process.env, CARGO_ENCODED_RUSTFLAGS: flags.join('\x1f') },
});
