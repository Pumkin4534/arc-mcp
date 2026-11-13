import assert from 'node:assert/strict';
import * as fileSystem from 'node:fs/promises';
import { homedir } from 'node:os';
import { dirname, join, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { randomUUID, createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { parse, modify, applyEdits } from 'jsonc-parser';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const equal = (a, b) => a === null || b === null ? a === b : a.equals(b);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
function decode(text) {
  const errors = [];
  const value = parse(text, errors, { allowTrailingComma:true });
  assert.equal(errors.length, 0, 'Invalid JSONC config; no installation changes made');
  assert.ok(value && !Array.isArray(value) && typeof value === 'object', 'Config must be an object');
  return value;
}
function numericVersion(version) {
  assert.match(version, /^\d+\.\d+\.\d+$/, 'Expected a stable release version');
  return version.split('.').map(Number);
}
function newer(a, b) {
  const left = numericVersion(a), right = numericVersion(b);
  for (let i=0; i<3; i++) if (left[i] !== right[i]) return left[i] > right[i];
  return false;
}

// fs is injectable for real-filesystem failure/concurrency tests, not a CLI option.
export async function installArc({
  configHome = resolve(process.env.ARC_CONFIG_HOME ?? join(process.env.XDG_CONFIG_HOME ?? join(homedir(), '.config'), 'opencode')),
  packageRoot = root,
  sourceBinary = process.env.ARC_INSTALL_BINARY,
  apply = false,
  fs = fileSystem,
  log = console.log,
} = {}) {
  configHome = resolve(configHome);
  const executable = process.platform === 'win32' ? 'arc-mcp.exe' : 'arc-mcp';
  const version = JSON.parse(await fs.readFile(join(packageRoot, 'package.json'), 'utf8')).version;
  numericVersion(version);
  const binary = join(configHome, 'arc-mcp/bin', executable);
  const bundled = join(packageRoot, 'bin', executable);
  if (!sourceBinary) {
    const present = await fs.lstat(bundled).then(() => true, e => { if(e.code === 'ENOENT') return false; throw e; });
    sourceBinary = present ? bundled : join(packageRoot, 'target/release', executable);
  }
  assert.equal(execFileSync(resolve(sourceBinary), ['--version'], { encoding:'utf8', timeout:10000 }).trim(), `arc-mcp ${version}`, 'Unexpected binary version');

  async function noSymlinkAncestors(path) {
    for (let p=dirname(path); p!==dirname(p); p=dirname(p)) {
      try { assert.ok(!(await fs.lstat(p)).isSymbolicLink(), `Refusing symlink ancestor: ${p}`); }
      catch(e) { if(e.code !== 'ENOENT') throw e; }
    }
  }
  async function snapshot(path) {
    await noSymlinkAncestors(path);
    try {
      const meta = await fs.lstat(path);
      assert.ok(meta.isFile() && !meta.isSymbolicLink(), `Refusing non-regular file: ${path}`);
      return { bytes:await fs.readFile(path), mode:meta.mode & 0o777 };
    } catch(e) { if(e.code === 'ENOENT') return { bytes:null, mode:0o600 }; throw e; }
  }
  async function unchanged(file) {
    // Never include config contents in assertion errors: they may contain secrets.
    assert.ok(equal((await snapshot(file.path)).bytes, file.before.bytes), `File changed during installation: ${file.path}`);
  }
  const jsonPath=join(configHome,'opencode.json'), jsoncPath=join(configHome,'opencode.jsonc');
  const json=await snapshot(jsonPath), jsonc=await snapshot(jsoncPath);
  assert.ok(!(json.bytes && jsonc.bytes), 'Both opencode.json and opencode.jsonc exist. Choose the active config explicitly before installing.');
  const configPath=json.bytes ? jsonPath : jsoncPath, before=json.bytes ? json : jsonc;
  const original=before.bytes?.toString('utf8') ?? '{\n  "$schema": "https://opencode.ai/config.json"\n}\n';
  const config=decode(original);
  assert.ok(config.mcp === undefined || (config.mcp && typeof config.mcp === 'object' && !Array.isArray(config.mcp)), 'Invalid mcp config');
  const previous=config.mcp?.arc;
  if (previous !== undefined) {
    assert.ok(previous && typeof previous === 'object' && !Array.isArray(previous), 'Invalid Arc config');
    assert.ok(previous.type === undefined || previous.type === 'local', 'The arc entry is not a local Arc installation');
    if (previous.command !== undefined) {
      assert.ok(Array.isArray(previous.command) && previous.command.every(v=>typeof v==='string'), 'Invalid Arc command');
      assert.match(previous.command[0] ?? '', /(?:^|[\\/])arc-mcp(?:\.exe)?$/, 'The arc command does not identify arc-mcp; inspect the conflicting entry');
      assert.ok(previous.command.slice(1).includes('serve'), 'Existing Arc command is not an MCP server');
    }
  }
  const args=previous?.command?.slice(1) ?? ['serve'];
  if (process.platform !== 'darwin' && !args.some(arg=>arg==='--runner' || arg.startsWith('--runner='))) args.push('--runner','native');
  const arc={ type:'local', enabled:true, timeout:780000, ...previous, command:[binary,...args] };
  let updated=original;
  // Modify individual fields so comments and custom Arc settings survive too.
  for (const [key,value] of Object.entries(arc)) if (!isDeepStrictEqual(previous?.[key],value)) {
    updated=applyEdits(updated, modify(updated,['mcp','arc',key],value,{formattingOptions:{insertSpaces:true,tabSize:2,eol:'\n'}}));
  }
  assert.ok(isDeepStrictEqual(decode(updated),{...config,mcp:{...config.mcp,arc}}), 'Config merge verification failed');

  const files=[{path:binary,bytes:await fs.readFile(sourceBinary),mode:0o755}];
  for (const [source,destination] of [
    ['arc-opencode.js','plugins/arc-opencode.js'],
    ...['agents/arc-reviewer.md','skills/arc-workflow/SKILL.md','commands/arc.md','commands/arc-status.md','commands/arc-continue.md'].map(p=>[p,p]),
  ]) files.push({path:join(configHome,destination),bytes:await fs.readFile(join(packageRoot,'integration',source)),mode:0o644});
  const receiptPath=join(configHome,'arc-mcp/install.json');
  const receiptBefore=await snapshot(receiptPath);
  if (receiptBefore.bytes) {
    const receipt=JSON.parse(receiptBefore.bytes.toString('utf8'));
    assert.equal(receipt.format,1,'Unknown installation receipt format');
    assert.ok(!newer(receipt.version,version), `Refusing to downgrade Arc ${receipt.version} to ${version}`);
  }
  const receipt={format:1,version,files:files.map(f=>({path:relative(configHome,f.path),sha256:hash(f.bytes)}))};
  files.push({path:receiptPath,bytes:Buffer.from(JSON.stringify(receipt,null,2)+'\n'),mode:0o600});
  for (const file of files) file.before=await snapshot(file.path);
  // Activate the configuration last, after all runtime resources are installed.
  files.push({path:configPath,bytes:Buffer.from(updated),mode:before.bytes ? before.mode : 0o600,before});
  const changes=files.filter(f=>!equal(f.before.bytes,f.bytes) || (process.platform!=='win32' && f.path===binary && (f.before.mode & 0o111)===0));
  if (!apply) { log(`Preflight passed: ${changes.length} Arc installation files need updating. Use --install to apply.`); return {status:'preflight',version,changes:changes.length}; }

  const lockPath=join(configHome,'arc-mcp/install.lock'), token=randomUUID();
  await noSymlinkAncestors(lockPath);
  await fs.mkdir(dirname(lockPath),{recursive:true,mode:0o700});
  try { await fs.writeFile(lockPath,JSON.stringify({token,pid:process.pid}),{flag:'wx',mode:0o600}); }
  catch(e) { if(e.code==='EEXIST') throw new Error(`Another installation or interrupted upgrade owns ${lockPath}. Inspect it before retrying; no upgrade was replayed.`); throw e; }
  let backup, journal;
  const activated=[], staged=[];
  const writeJournal=async phase=>{
    journal.phase=phase;
    const path=join(backup,'journal.json'), temp=join(backup,`journal-${token}.tmp`);
    await fs.writeFile(temp,JSON.stringify(journal,null,2)+'\n',{flag:'wx',mode:0o600});
    await fs.rename(temp,path);
  };
  try {
    for (const file of files) await unchanged(file);
    // Also guard against a second config appearing after preflight.
    assert.equal((await snapshot(configPath===jsonPath ? jsoncPath : jsonPath)).bytes,null,'A second OpenCode config appeared during installation');
    if (!changes.length) { log(`Arc MCP ${version} is already up to date.`); return {status:'current',version,changes:0}; }
    backup=join(configHome,'arc-mcp/backups',`${new Date().toISOString().replaceAll(':','-')}-${token}`);
    await noSymlinkAncestors(join(backup,'journal.json'));
    await fs.mkdir(backup,{recursive:true,mode:0o700});
    journal={format:1,version,phase:'preparing',files:files.map(f=>({path:relative(configHome,f.path),existed:f.before.bytes!==null,mode:f.before.mode,sha256:f.before.bytes ? hash(f.before.bytes) : null}))};
    for (const file of files) if (file.before.bytes !== null) {
      const path=join(backup,'files',relative(configHome,file.path));
      await fs.mkdir(dirname(path),{recursive:true,mode:0o700});
      await fs.writeFile(path,file.before.bytes,{flag:'wx',mode:0o600});
    }
    await writeJournal('prepared');
    log(`Arc rollback backup: ${backup}`);
    for (const file of changes) {
      await fs.mkdir(dirname(file.path),{recursive:true});
      const path=join(backup,'incoming',relative(configHome,file.path));
      await fs.mkdir(dirname(path),{recursive:true,mode:0o700});
      await fs.writeFile(path,file.bytes,{flag:'wx',mode:file.mode});
      staged.push(path); file.staged=path;
    }
    await writeJournal('installing');
    for (const file of changes) {
      await unchanged(file);
      const entry={file,moved:false,installed:false,displaced:join(backup,'displaced',relative(configHome,file.path))};
      activated.push(entry);
      if (file.before.bytes !== null) {
        await fs.mkdir(dirname(entry.displaced),{recursive:true,mode:0o700});
        await fs.rename(file.path,entry.displaced); entry.moved=true;
      }
      await fs.rename(file.staged,file.path); entry.installed=true;
    }
    await writeJournal('complete');
    log(`Arc MCP ${version} installed/upgraded. Quit and restart OpenCode, then use /arc.`);
    return {status:'installed',version,changes:changes.length,backup};
  } catch(error) {
    const failures=[];
    for (const entry of activated.reverse()) {
      try {
        if (entry.installed) {
          assert.ok(equal((await snapshot(entry.file.path)).bytes,entry.file.bytes),`Concurrent edit preserved: ${entry.file.path}`);
          await fs.unlink(entry.file.path);
        }
        if (entry.moved) {
          assert.equal((await snapshot(entry.file.path)).bytes,null,`Concurrent replacement preserved: ${entry.file.path}`);
          await fs.rename(entry.displaced,entry.file.path);
        }
      } catch(e) { failures.push(e.message); }
    }
    if (journal) await writeJournal(failures.length ? 'rollback-incomplete' : 'rolled-back').catch(e=>failures.push(e.message));
    if (failures.length) throw new Error(`Upgrade failed; rollback needs inspection at ${backup}: ${failures.join('; ')}`,{cause:error});
    throw error;
  } finally {
    for (const path of staged) await fs.unlink(path).catch(e=>{if(e.code!=='ENOENT') log(`Inspect leftover staged file: ${path}`);});
    // Do not remove a lock replaced by a concurrent actor.
    const lock=JSON.parse(await fs.readFile(lockPath,'utf8'));
    if (lock.token===token) await fs.unlink(lockPath);
  }
}

if (process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  await installArc({apply:process.argv.includes('--install')});
}
