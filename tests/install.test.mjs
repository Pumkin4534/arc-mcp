import test from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import { mkdtemp, mkdir, writeFile, readFile, readdir, realpath, symlink, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { parse } from 'jsonc-parser';
import { installArc } from '../scripts/install.mjs';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const executable = process.platform === 'win32' ? 'arc-mcp.exe' : 'arc-mcp';
const version = JSON.parse(await readFile(join(root, 'package.json'), 'utf8')).version;
const sourceBinary=process.env.ARC_TEST_INSTALL_BINARY ?? join(root,'target/release',executable);
async function fixture() {
  const home = await realpath(await mkdtemp(join(tmpdir(),'arc-installer-')));
  const config = join(home,'config/opencode');
  await mkdir(config,{recursive:true});
  const env = { ...process.env, HOME:home, ARC_CONFIG_HOME:config, ARC_INSTALL_BINARY:sourceBinary };
  const run = (...args) => execFileSync(process.execPath,[join(root,'scripts/install.mjs'),...args],{env,encoding:'utf8',stdio:['ignore','pipe','pipe']});
  return { home,config,run };
}
const backups=async f=>(await readdir(join(f.config,'arc-mcp/backups'))).sort();
const apply=(f,io=fs)=>installArc({configHome:f.config,sourceBinary,apply:true,fs:io,log:()=>{}});
test('fresh installation uses runtime paths, exact files and valid config', async t=>{
  const f=await fixture(); t.after(()=>rm(f.home,{recursive:true,force:true}));
  f.run('--install');
  const config=parse(await readFile(join(f.config,'opencode.jsonc'),'utf8'));
  assert.deepEqual(config.mcp.arc.command,[join(f.config,'arc-mcp/bin',executable),'serve',...(process.platform === 'darwin' ? [] : ['--runner','native'])]);
  assert.equal(execFileSync(config.mcp.arc.command[0],['--version'],{encoding:'utf8'}).trim(),`arc-mcp ${version}`);
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
  const saved=await backups(f);
  assert.equal(saved.length,1);
  assert.equal(await readFile(join(f.config,'arc-mcp/backups',saved[0],'files/opencode.jsonc'),'utf8'),original);
  f.run('--install');
  assert.equal(await readFile(join(f.config,'opencode.jsonc'),'utf8'),updated);
});
test('existing JSON config is edited instead of shadowed by JSONC', async t=>{
  const f=await fixture(); t.after(()=>rm(f.home,{recursive:true,force:true}));
  await writeFile(join(f.config,'opencode.json'),'{}'); f.run('--install');
  assert.ok(parse(await readFile(join(f.config,'opencode.json'),'utf8')).mcp.arc);
  assert.ok(!(await readdir(f.config)).includes('opencode.jsonc'));
});
for(const scenario of ['conflicting-arc','invalid-json','two-configs','symlink-plugin']) test(`${scenario} refuses before activation`,async t=>{
  const f=await fixture(); t.after(()=>rm(f.home,{recursive:true,force:true}));
  const original=scenario==='invalid-json'?'{broken':scenario==='conflicting-arc'?'{"mcp":{"arc":{"type":"remote","url":"https://example.invalid"}}}':'{}';
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

const managed=[`arc-mcp/bin/${executable}`,'plugins/arc-opencode.js','agents/arc-reviewer.md','skills/arc-workflow/SKILL.md','commands/arc.md','commands/arc-status.md','commands/arc-continue.md'];
async function legacy(f) {
  const old={permission:{bash:'ask'},provider:{example:{options:{setting:'keep'}}},mcp:{other:{type:'local',command:['other-tool']},arc:{type:'local',command:[join(f.home,'.cargo/bin',executable),'serve','--data-home',join(f.home,'evidence')],enabled:false,timeout:123456,environment:{EXAMPLE:'keep'}}}};
  const text='// Retain configuration comments\n'+JSON.stringify(old,null,2).replace('"command": [\n        "'+join(f.home,'.cargo/bin',executable).replaceAll('\\','\\\\'), '// Retain Arc comment\n      "command": [\n        "'+join(f.home,'.cargo/bin',executable).replaceAll('\\','\\\\'))+'\n';
  await writeFile(join(f.config,'opencode.jsonc'),text);
  for(const path of managed){await mkdir(dirname(join(f.config,path)),{recursive:true});await writeFile(join(f.config,path),`previous contents: ${path}`,{mode:path.includes('/bin/')?0o755:0o600});}
  await mkdir(join(f.home,'evidence'));await writeFile(join(f.home,'evidence','history.json'),'retained workflow history');
  const before=new Map();for(const p of [...managed,'opencode.jsonc'])before.set(p,await readFile(join(f.config,p)));
  return {old,text,before};
}
async function assertRestored(f,before) {for(const [p,bytes]of before)assert.deepEqual(await readFile(join(f.config,p)),bytes,p);}

test('legacy upgrade replaces Arc files, backs up originals and preserves settings/history',async t=>{
  const f=await fixture();t.after(()=>rm(f.home,{recursive:true,force:true}));const {old,text,before}=await legacy(f);
  f.run('--install');
  const updatedText=await readFile(join(f.config,'opencode.jsonc'),'utf8'),updated=parse(updatedText);
  assert.ok(updatedText.includes('// Retain configuration comments'));
  assert.ok(updatedText.includes('// Retain Arc comment'));
  const args=[...old.mcp.arc.command.slice(1),...(process.platform==='darwin'?[]:['--runner','native'])];
  assert.deepEqual(updated,{...old,mcp:{...old.mcp,arc:{...old.mcp.arc,command:[join(f.config,'arc-mcp/bin',executable),...args]}}});
  assert.equal(execFileSync(updated.mcp.arc.command[0],['--version'],{encoding:'utf8'}).trim(),`arc-mcp ${version}`);
  assert.deepEqual(await readFile(join(f.config,'plugins/arc-opencode.js')),await readFile(join(root,'integration/arc-opencode.js')));
  assert.equal(await readFile(join(f.home,'evidence/history.json'),'utf8'),'retained workflow history');
  const saved=await backups(f);assert.equal(saved.length,1);
  const backup=join(f.config,'arc-mcp/backups',saved[0]);
  for(const [p,bytes]of before)assert.deepEqual(await readFile(join(backup,'files',p)),bytes);
  assert.equal(JSON.parse(await readFile(join(backup,'journal.json'),'utf8')).phase,'complete');
  if(process.platform!=='win32')assert.equal((await fs.stat(join(backup,'files/opencode.jsonc'))).mode&0o777,0o600);
  assert.notEqual(updatedText,text);
  f.run('--install');assert.deepEqual(await backups(f),saved);assert.equal(await readFile(join(f.config,'opencode.jsonc'),'utf8'),updatedText);
});

test('same-version rerun repairs changed Arc resources and retains the edited originals',async t=>{
  const f=await fixture();t.after(()=>rm(f.home,{recursive:true,force:true}));f.run('--install');
  await writeFile(join(f.config,'plugins/arc-opencode.js'),'custom previous Arc plugin');
  const result=await apply(f);assert.equal(result.status,'installed');
  assert.equal(await readFile(join(result.backup,'files/plugins/arc-opencode.js'),'utf8'),'custom previous Arc plugin');
  assert.deepEqual(await readFile(join(f.config,'plugins/arc-opencode.js')),await readFile(join(root,'integration/arc-opencode.js')));
  assert.equal((await apply(f)).status,'current');
});

test('mid-upgrade activation failure rolls back binary, plugin and config',async t=>{
  const f=await fixture();t.after(()=>rm(f.home,{recursive:true,force:true}));const {before}=await legacy(f);
  let injected=false;
  const io={...fs,rename:async(from,to)=>{if(!injected&&to===join(f.config,'agents/arc-reviewer.md')){injected=true;throw new Error('injected activation failure');}return fs.rename(from,to);}};
  await assert.rejects(apply(f,io),/injected activation failure/);assert.ok(injected);
  await assertRestored(f,before);await assert.rejects(fs.access(join(f.config,'arc-mcp/install.lock')));
  await assert.rejects(fs.access(join(f.config,'arc-mcp/install.json')));
  const [saved]=await backups(f);assert.equal(JSON.parse(await readFile(join(f.config,'arc-mcp/backups',saved,'journal.json'),'utf8')).phase,'rolled-back');
});

test('a concurrent config edit is preserved while owned runtime changes roll back',async t=>{
  const f=await fixture();t.after(()=>rm(f.home,{recursive:true,force:true}));const {before,text}=await legacy(f);
  const concurrent=text+'// External edit during installation\n';let edited=false;
  const io={...fs,rename:async(from,to)=>{await fs.rename(from,to);if(!edited&&to===join(f.config,'arc-mcp/bin',executable)){edited=true;await writeFile(join(f.config,'opencode.jsonc'),concurrent);}}};
  await assert.rejects(apply(f,io),/File changed during installation/);
  before.set('opencode.jsonc',Buffer.from(concurrent));await assertRestored(f,before);
});

test('overlapping installers refuse without removing the active owner lock',async t=>{
  const f=await fixture();t.after(()=>rm(f.home,{recursive:true,force:true}));await legacy(f);
  let signal,release;const started=new Promise(r=>signal=r),gate=new Promise(r=>release=r);let held=false;
  const first=apply(f,{...fs,rename:async(from,to)=>{if(!held&&to===join(f.config,'arc-mcp/bin',executable)){held=true;signal();await gate;}return fs.rename(from,to);}});
  await started;
  try {assert.throws(()=>f.run('--install'),/Another installation or interrupted upgrade/);await fs.access(join(f.config,'arc-mcp/install.lock'));}
  finally {release();}
  assert.equal((await first).status,'installed');await assert.rejects(fs.access(join(f.config,'arc-mcp/install.lock')));
});

test('old installers refuse receipt-marked downgrades before changing files',async t=>{
  const f=await fixture();t.after(()=>rm(f.home,{recursive:true,force:true}));f.run('--install');
  const receiptPath=join(f.config,'arc-mcp/install.json'),receipt=JSON.parse(await readFile(receiptPath,'utf8'));receipt.version='999.0.0';await writeFile(receiptPath,JSON.stringify(receipt));
  const before=await readFile(join(f.config,'arc-mcp/bin',executable)),saved=await backups(f);
  assert.throws(()=>f.run('--install'),/Refusing to downgrade/);
  assert.deepEqual(await readFile(join(f.config,'arc-mcp/bin',executable)),before);assert.deepEqual(await backups(f),saved);
});
