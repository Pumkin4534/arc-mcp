// Downloads the official pinned CLI for isolated transport tests, not inference.
import { mkdir, writeFile, readdir, chmod, appendFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const directory = join(root, 'target/test-opencode');
await mkdir(directory, { recursive: true });
const platform = { darwin: 'darwin', linux: 'linux', win32: 'windows' }[process.platform];
if (!platform) throw new Error('Unsupported test platform');
const extension = process.platform === 'linux' ? 'tar.gz' : 'zip';
const asset = `opencode-${platform}-${process.arch}${process.arch === 'x64' ? '-baseline' : ''}.${extension}`;
const response = await fetch(`https://github.com/anomalyco/opencode/releases/download/v1.18.25/${asset}`, { signal: AbortSignal.timeout(120000) });
if (!response.ok) throw new Error(`Official test CLI unavailable: ${response.status}`);
const archive = join(directory, asset);
await writeFile(archive, Buffer.from(await response.arrayBuffer()));
if (process.platform === 'win32') {
  const quote = value => `'${value.replaceAll("'", "''")}'`;
  execFileSync('powershell.exe', ['-NoProfile','-NonInteractive','-Command',`Expand-Archive -LiteralPath ${quote(archive)} -DestinationPath ${quote(directory)} -Force`]);
} else if (extension === 'zip') execFileSync('unzip', ['-q','-o',archive,'-d',directory]);
else execFileSync('tar', ['-xzf',archive,'-C',directory]);
async function locate(dir) {
  for (const item of await readdir(dir, { withFileTypes:true })) {
    const path = join(dir,item.name);
    if (item.isFile() && item.name === (process.platform === 'win32' ? 'opencode.exe' : 'opencode')) return path;
    if (item.isDirectory()) { const found = await locate(path); if(found) return found; }
  }
}
const binary = await locate(directory);
if (!binary) throw new Error('Official archive contains no CLI');
await chmod(binary,0o755);
if (execFileSync(binary,['--version'],{encoding:'utf8'}).trim() !== '1.18.25') throw new Error('Unexpected official CLI version');
if(process.env.GITHUB_ENV) await appendFile(process.env.GITHUB_ENV, `ARC_TEST_OPENCODE=${binary}\nARC_TEST_BINARY=${join(root,'target/release',process.platform==='win32'?'arc-mcp.exe':'arc-mcp')}\nARC_TEST_INSTALLER=1\n`);
console.log('Official OpenCode 1.18.25 ready for isolated tests');
