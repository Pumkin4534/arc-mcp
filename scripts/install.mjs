import assert from 'node:assert/strict';
import { readFile, mkdir, lstat, writeFile, rename, unlink } from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { parse, modify, applyEdits } from 'jsonc-parser';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const configHome = resolve(process.env.ARC_CONFIG_HOME ?? join(process.env.XDG_CONFIG_HOME ?? join(homedir(), '.config'), 'opencode'));
const binary = join(configHome, 'arc-mcp/bin/arc-mcp');
const sourceBinary = resolve(process.env.ARC_INSTALL_BINARY ?? join(root, 'target/release/arc-mcp'));
assert.equal(execFileSync(sourceBinary, ['--version'], { encoding:'utf8' }).trim(), 'arc-mcp 0.1.1', 'Unexpected binary version');

async function existing(path) {
  try {
    const meta = await lstat(path);
    assert.ok(meta.isFile() && !meta.isSymbolicLink(), `Refusing non-regular file: ${path}`);
    return await readFile(path);
  } catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}
async function noSymlinkAncestors(path) {
  for (let p = dirname(path); p !== dirname(p); p = dirname(p)) {
    try { assert.ok(!(await lstat(p)).isSymbolicLink(), `Refusing symlink ancestor: ${p}`); }
    catch(error) { if(error.code !== 'ENOENT') throw error; }
  }
}
const jsonPath = join(configHome, 'opencode.json');
const jsoncPath = join(configHome, 'opencode.jsonc');
const jsonBytes = await existing(jsonPath);
const jsoncBytes = await existing(jsoncPath);
assert.ok(!(jsonBytes && jsoncBytes), 'Both opencode.json and opencode.jsonc exist. Choose the active config explicitly before installing.');
const configPath = jsonBytes ? jsonPath : jsoncPath;
const before = jsonBytes ?? jsoncBytes;
const original = before?.toString('utf8') ?? '{\n  "$schema": "https://opencode.ai/config.json"\n}\n';
function decode(text) {
  const errors = [];
  const result = parse(text, errors, { allowTrailingComma:true });
  assert.equal(errors.length, 0, 'Invalid JSONC config; no files changed');
  assert.ok(result && !Array.isArray(result) && typeof result === 'object', 'Config must be an object');
  return result;
}
const config = decode(original);
assert.ok(config.mcp === undefined || (config.mcp && typeof config.mcp === 'object' && !Array.isArray(config.mcp)), 'Invalid mcp config');
const arc = { type:'local', command:[binary, 'serve'], enabled:true, timeout:780000 };
if(config.mcp?.arc) assert.deepEqual(config.mcp.arc, arc, 'A different Arc installation already exists; no replacement was made');
const updated = config.mcp?.arc ? original : applyEdits(original, modify(original, ['mcp','arc'], arc, { formattingOptions:{ insertSpaces:true, tabSize:2, eol:'\n' } }));
assert.deepEqual(decode(updated), { ...config, mcp:{ ...config.mcp, arc } });
const files = [{ path:binary, bytes:await readFile(sourceBinary), mode:0o755 }];
for(const [source, destination] of [
  ['arc-opencode.js','plugins/arc-opencode.js'],
  ...['agents/arc-reviewer.md','skills/arc-workflow/SKILL.md','commands/arc.md','commands/arc-status.md','commands/arc-continue.md'].map(path=>[path,path]),
]) files.push({ path:join(configHome,destination), bytes:await readFile(join(root,'integration',source)), mode:0o644 });
for(const file of files) {
  await noSymlinkAncestors(file.path);
  file.before = await existing(file.path);
  if(file.before) assert.deepEqual(file.before, file.bytes, `Existing Arc file differs; preserving it: ${file.path}`);
}
await noSymlinkAncestors(configPath);
if(!process.argv.includes('--install')) {
  console.log('Preflight passed. Use --install to apply; existing settings will be preserved.');
} else {
  assert.deepEqual(await existing(configPath), before, 'Config changed during preflight');
  const created = [];
  let staged;
  try {
    await mkdir(configHome,{recursive:true});
    if(before && updated !== original) {
      const backup = `${configPath}.before-arc-${randomUUID()}.bak`;
      await writeFile(backup,before,{flag:'wx',mode:0o600});
      console.log(`Config backup: ${backup}`);
    }
    for(const file of files) {
      if(file.before) continue;
      await mkdir(dirname(file.path),{recursive:true});
      await writeFile(file.path,file.bytes,{flag:'wx',mode:file.mode});
      created.push(file.path);
    }
    if(!before || updated !== original) {
      staged = `${configPath}.${randomUUID()}.tmp`;
      await writeFile(staged,updated,{flag:'wx',mode:0o600});
      assert.deepEqual(await existing(configPath),before,'Config changed before activation');
      await rename(staged,configPath);
      staged = undefined;
    }
    console.log('Arc MCP installed. Quit and restart OpenCode, then use /arc.');
  } catch(error) {
    if(staged) await unlink(staged).catch(()=>{});
    for(const path of created.reverse()) await unlink(path).catch(()=>{});
    throw error;
  }
}
