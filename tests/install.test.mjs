import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, readdir, realpath, symlink, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { parse } from 'jsonc-parser';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
async function fixture() {
  const home = await realpath(await mkdtemp(join(tmpdir(),'arc-installer-')));
  const config = join(home,'config/opencode');
  await mkdir(config,{recursive:true});
  const env = { ...process.env, HOME:home, ARC_CONFIG_HOME:config, ARC_INSTALL_BINARY:join(root,'target/release/arc-mcp') };
  const run = (...args) => execFileSync(process.execPath,[join(root,'scripts/install.mjs'),...args],{env,encoding:'utf8',stdio:['ignore','pipe','pipe']});
  return { home,config,run };
}
test('fresh installation uses runtime paths, exact files and valid config', async t=>{
  const f=await fixture(); t.after(()=>rm(f.home,{recursive:true,force:true}));
  f.run('--install');
  const config=parse(await readFile(join(f.config,'opencode.jsonc'),'utf8'));
  assert.deepEqual(config.mcp.arc.command,[join(f.config,'arc-mcp/bin/arc-mcp'),'serve']);
  assert.equal(execFileSync(config.mcp.arc.command[0],['--version'],{encoding:'utf8'}).trim(),'arc-mcp 0.1.1');
  assert.deepEqual(await readFile(join(f.config,'plugins/arc-opencode.js')),await readFile(join(root,'integration/arc-opencode.js')));
  f.run('--install');
  assert.equal((await readdir(f.config)).filter(name=>name.endsWith('.bak')).length,0);
});
test('JSONC comments, trailing commas, unrelated services and permissions survive', async t=>{
  const f=await fixture(); t.after(()=>rm(f.home,{recursive:true,force:true}));
  const original='// Keep this note\n{\n "mcp": {"existing": {"type":"local","command":["service"],},},\n "permission":{"bash":"ask"},\n}\n';
  await writeFile(join(f.config,'opencode.jsonc'),original);
  f.run('--install');
  const updated=await readFile(join(f.config,'opencode.jsonc'),'utf8');
  assert.ok(updated.startsWith('// Keep this note'));
  const config=parse(updated); delete config.mcp.arc;
  assert.deepEqual(config,parse(original));
  const backups=(await readdir(f.config)).filter(name=>name.endsWith('.bak'));
  assert.equal(backups.length,1);
  assert.equal(await readFile(join(f.config,backups[0]),'utf8'),original);
  f.run('--install');
  assert.equal(await readFile(join(f.config,'opencode.jsonc'),'utf8'),updated);
});
test('existing JSON config is edited instead of shadowed by JSONC', async t=>{
  const f=await fixture(); t.after(()=>rm(f.home,{recursive:true,force:true}));
  await writeFile(join(f.config,'opencode.json'),'{}'); f.run('--install');
  assert.ok(parse(await readFile(join(f.config,'opencode.json'),'utf8')).mcp.arc);
  assert.ok(!(await readdir(f.config)).includes('opencode.jsonc'));
});
for(const scenario of ['conflicting-arc','invalid-json','conflicting-plugin','two-configs','symlink-plugin']) test(`${scenario} refuses before activation`,async t=>{
  const f=await fixture(); t.after(()=>rm(f.home,{recursive:true,force:true}));
  const original=scenario==='invalid-json'?'{broken':scenario==='conflicting-arc'?'{"mcp":{"arc":{"enabled":false}}}':'{}';
  await writeFile(join(f.config,'opencode.jsonc'),original);
  if(scenario==='two-configs') await writeFile(join(f.config,'opencode.json'),'{}');
  if(scenario.includes('plugin')) {
    await mkdir(join(f.config,'plugins'));
    if(scenario==='symlink-plugin') { await writeFile(join(f.home,'original'),'keep'); await symlink(join(f.home,'original'),join(f.config,'plugins/arc-opencode.js')); }
    else await writeFile(join(f.config,'plugins/arc-opencode.js'),'user-edited plugin');
  }
  assert.throws(()=>f.run('--install'));
  assert.equal(await readFile(join(f.config,'opencode.jsonc'),'utf8'),original);
  assert.ok(!(await readdir(f.config)).includes('arc-mcp'));
});
