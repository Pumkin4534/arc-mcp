// Exercises the packaged binary + installer + a real platform check. No model.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const {version}=JSON.parse(await readFile(join(root,'package.json'),'utf8'));
const platform={darwin:'macos',linux:'linux',win32:'windows'}[process.platform];
const name=`arc-mcp-${version}-${platform}-${process.arch}`;
const fixture=await mkdtemp(join(root,'target/bundle-smoke-'));
const unpacked=join(fixture,'unpacked'),home=join(fixture,'home'),project=join(fixture,'project');
await Promise.all([mkdir(unpacked),mkdir(home),mkdir(project)]);
const archive=join(root,'dist',`${name}.${process.platform==='win32'?'zip':'tar.gz'}`);
if(process.platform==='win32'){
 const quote=s=>`'${s.replaceAll("'","''")}'`;
 execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',`Expand-Archive -LiteralPath ${quote(archive)} -DestinationPath ${quote(unpacked)}`]);
}else execFileSync('tar',['-xzf',archive,'-C',unpacked]);
const payload=join(unpacked,name),configHome=join(home,'.config/opencode');
const env={...process.env,HOME:home,USERPROFILE:home,ARC_CONFIG_HOME:configHome,ARC_DATA_HOME:join(fixture,'state')};
if(process.platform==='win32'){env.APPDATA=join(home,'AppData/Roaming');env.LOCALAPPDATA=join(home,'AppData/Local')}
if(process.platform==='win32')execFileSync('cmd.exe',['/d','/s','/c','npm.cmd ci --ignore-scripts --no-audit --no-fund'],{cwd:payload,env,stdio:'inherit'});
else execFileSync('npm',['ci','--ignore-scripts','--no-audit','--no-fund'],{cwd:payload,env,stdio:'inherit'});
execFileSync(process.execPath,[join(payload,'scripts/install.mjs'),'--install'],{env,stdio:'inherit'});
const config=JSON.parse(await readFile(join(configHome,'opencode.jsonc'),'utf8'));
const [binary,...args]=config.mcp.arc.command;
assert.equal(execFileSync(binary,['--version'],{env,encoding:'utf8'}).trim(),`arc-mcp ${version}`);
await writeFile(join(project,'answer.txt'),'42\n');
const child=spawn(binary,[...args,'--root',project],{cwd:project,env,stdio:['pipe','pipe','pipe']});
const lines=createInterface({input:child.stdout});const pending=new Map();let next=1,stderr='';
child.stderr.on('data',value=>stderr+=value);
lines.on('line',line=>{const data=JSON.parse(line);const waiting=pending.get(data.id);if(waiting){pending.delete(data.id);waiting.resolve(data)}});
child.on('error',error=>{for(const p of pending.values())p.reject(error)});
child.on('exit',code=>{for(const p of pending.values())p.reject(new Error(`MCP exited ${code}: ${stderr}`))});
async function rpc(method,params={}){const id=next++;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(new Error(`RPC timeout: ${method}`)),60000);pending.set(id,{resolve:value=>{clearTimeout(timer);resolve(value)},reject:error=>{clearTimeout(timer);reject(error)}});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n')})}
async function tool(name,arguments_){const response=await rpc('tools/call',{name,arguments:arguments_});assert.ok(!response.error,JSON.stringify(response));assert.equal(response.result.isError,false,JSON.stringify(response));return response.result.structuredContent}
try{
 const init=await rpc('initialize',{protocolVersion:'2025-11-25',clientInfo:{name:'bundle-smoke',version:'1'},capabilities:{}});assert.equal(init.result.serverInfo.version,version);
 child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
 assert.equal((await rpc('tools/list')).result.tools.length,7);
 await tool('arc_plan',{goal:'Verify bundled runtime',tasks:[{id:'check',title:'Verify answer',acceptance:['answer.txt is 42'],depends_on:[]}]});
 const command=process.platform==='win32'?"if ((Get-Content -Raw answer.txt).Trim() -ne '42') {exit 1}; Write-Output 'verified'":'test "$(cat answer.txt)" = 42 && printf verified';
 const result=await tool('arc_run_check',{task_ids:['check'],command,timeout_secs:30});
 assert.equal(result.artifact.passed,true,JSON.stringify(result));
 if(process.platform!=='darwin')assert.match(result.artifact.outcome.runner,/NO filesystem\/network sandbox/);
 console.log(JSON.stringify({passed:true,platform,arch:process.arch,version,packagedInstaller:true,mcpTools:7,realCommandPassed:true,assurance:result.assurance}));
}finally{child.stdin.end();lines.close();const timer=setTimeout(()=>child.kill('SIGKILL'),3000);timer.unref();await new Promise(done=>child.exitCode!==null?done():child.once('exit',done));clearTimeout(timer)}
