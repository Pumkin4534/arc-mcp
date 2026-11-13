// Real previous-release binary/installer -> current bundle -> identical rerun.
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,readFile,writeFile,readdir} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {parse,modify,applyEdits} from 'jsonc-parser';
import {windowsPowerShell} from '../scripts/windows-powershell.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const {version}=JSON.parse(await readFile(join(root,'package.json'),'utf8'));
const previous='0.2.0',platform={darwin:'macos',linux:'linux',win32:'windows'}[process.platform],extension=process.platform==='win32'?'zip':'tar.gz';
assert.notEqual(version,previous,'Upgrade test needs a newer release');
const fixture=await mkdtemp(join(root,'target/upgrade-smoke-'));
const home=join(fixture,'home with spaces'),configHome=join(home,'.config/opencode');
await mkdir(configHome,{recursive:true});
const env={...process.env,HOME:home,USERPROFILE:home,ARC_CONFIG_HOME:configHome,ARC_DATA_HOME:join(fixture,'history')};
if(process.platform==='win32'){env.APPDATA=join(home,'AppData/Roaming');env.LOCALAPPDATA=join(home,'AppData/Local');}
await mkdir(env.ARC_DATA_HOME);await writeFile(join(env.ARC_DATA_HOME,'keep.txt'),'retain evidence');
await writeFile(join(configHome,'opencode.jsonc'),'// User settings survive\n{"mcp":{"keep":{"type":"local","command":["keep"]}},"permission":{"bash":"ask"}}\n');
const asset=v=>`arc-mcp-${v}-${platform}-${process.arch}.${extension}`;
const oldArchive=join(fixture,asset(previous));
const release=`https://github.com/Pumkin4534/arc-mcp/releases/download/v${previous}`;
const sumResponse=await fetch(`${release}/${asset(previous)}.sha256`,{signal:AbortSignal.timeout(60000)});assert.equal(sumResponse.status,200);
const [expected,filename]=(await sumResponse.text()).trim().split(/\s+/);assert.equal(filename,asset(previous));assert.match(expected,/^[a-f0-9]{64}$/);
const response=await fetch(`${release}/${asset(previous)}`,{signal:AbortSignal.timeout(60000)});assert.equal(response.status,200);
const bytes=Buffer.from(await response.arrayBuffer());assert.equal(createHash('sha256').update(bytes).digest('hex'),expected);await writeFile(oldArchive,bytes);
async function unpack(archive,v){
 const dir=join(fixture,`unpacked-${v}`);await mkdir(dir);
 if(process.platform==='win32'){
  const q=s=>`'${s.replaceAll("'","''")}'`;
  windowsPowerShell(`Expand-Archive -LiteralPath ${q(archive)} -DestinationPath ${q(dir)}`,{stdio:'inherit',timeout:120000});
 }else execFileSync('tar',['-xzf',archive,'-C',dir]);
 return join(dir,`arc-mcp-${v}-${platform}-${process.arch}`);
}
function install(payload){
 if(process.platform==='win32')execFileSync('cmd.exe',['/d','/s','/c','npm.cmd ci --ignore-scripts --no-audit --no-fund'],{cwd:payload,env,stdio:'inherit',timeout:120000});
 else execFileSync('npm',['ci','--ignore-scripts','--no-audit','--no-fund'],{cwd:payload,env,stdio:'inherit',timeout:120000});
 execFileSync(process.execPath,[join(payload,'scripts/install.mjs'),'--install'],{cwd:payload,env,stdio:'inherit',timeout:60000});
}
install(await unpack(oldArchive,previous));
const configPath=join(configHome,'opencode.jsonc');let original=await readFile(configPath,'utf8');
const binary=parse(original).mcp.arc.command[0];assert.equal(execFileSync(binary,['--version'],{encoding:'utf8'}).trim(),`arc-mcp ${previous}`);
original=applyEdits(original,modify(original,['mcp','arc','timeout'],123456,{}));await writeFile(configPath,original);
const oldBinary=await readFile(binary),oldPlugin=await readFile(join(configHome,'plugins/arc-opencode.js'));
const current=await unpack(join(root,'dist',asset(version)),version);install(current);
assert.equal(execFileSync(binary,['--version'],{encoding:'utf8'}).trim(),`arc-mcp ${version}`);
assert.equal(await readFile(configPath,'utf8'),original,'Unchanged settings retain exact JSONC bytes');
assert.equal(await readFile(join(env.ARC_DATA_HOME,'keep.txt'),'utf8'),'retain evidence');
const backupRoot=join(configHome,'arc-mcp/backups'),saved=await readdir(backupRoot);assert.equal(saved.length,1);
const backup=join(backupRoot,saved[0]);
assert.deepEqual(await readFile(join(backup,'files/arc-mcp/bin',process.platform==='win32'?'arc-mcp.exe':'arc-mcp')),oldBinary);
assert.deepEqual(await readFile(join(backup,'files/plugins/arc-opencode.js')),oldPlugin);
assert.equal(await readFile(join(backup,'files/opencode.jsonc'),'utf8'),original);
assert.deepEqual(await readFile(join(configHome,'plugins/arc-opencode.js')),await readFile(join(current,'integration/arc-opencode.js')));
execFileSync(process.execPath,[join(current,'scripts/install.mjs'),'--install'],{env,stdio:'inherit',timeout:60000});
assert.deepEqual(await readdir(backupRoot),saved);assert.equal(await readFile(configPath,'utf8'),original);
console.log(JSON.stringify({passed:true,platform,arch:process.arch,from:previous,to:version,realPreviousRelease:true,backupVerified:true,settingsAndHistoryPreserved:true,repeatIsNoOp:true}));
