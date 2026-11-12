// Runs the actual OS bootstrap against a local fixture of the verified release
// download. Installer/config operations are real; only bundle transport is mocked.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp,mkdir,readFile,writeFile,access } from 'node:fs/promises';
import { dirname,join,resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const bootstrapNode=process.argv.includes('--bootstrap-node');
const {version}=JSON.parse(await readFile(join(root,'package.json'),'utf8'));
const platform={darwin:'macos',linux:'linux',win32:'windows'}[process.platform];
const asset=`arc-mcp-${version}-${platform}-${process.arch}.${process.platform==='win32'?'zip':'tar.gz'}`;
const release=`https://github.com/Pumkin4534/arc-mcp/releases/download/v${version}`;
const fixture=await mkdtemp(join(root,'target/bootstrap-smoke-'));
const home=join(fixture,'home'),bin=join(fixture,'bin'),tmp=join(fixture,'tmp');
await Promise.all([mkdir(home),mkdir(bin),mkdir(tmp)]);
const env={PATH:`${bin}${process.platform==='win32'?';':':'}${process.env.PATH}`,HOME:home,USERPROFILE:home,ARC_CONFIG_HOME:join(home,'.config/opencode'),ARC_TOOLS_HOME:join(home,'tools'),TMPDIR:tmp,TEMP:tmp,TMP:tmp};
for(const key of ['SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','SystemDrive','ProgramFiles','ProgramFiles(x86)','ProgramW6432','OS','PROCESSOR_ARCHITECTURE','PROCESSOR_ARCHITEW6432'])if(process.env[key])env[key]=process.env[key];
const archive=join(root,'dist',asset),checksum=`${archive}.sha256`;
if(process.platform==='win32'){
  env.APPDATA=join(home,'AppData/Roaming');env.LOCALAPPDATA=join(home,'AppData/Local');
  env.PSModulePath=join(env.SYSTEMROOT,'System32/WindowsPowerShell/v1.0/Modules');
  if(bootstrapNode)env.PATH=[bin,join(env.SYSTEMROOT,'System32'),env.SYSTEMROOT,join(env.SYSTEMROOT,'System32/WindowsPowerShell/v1.0')].join(';');
  const q=s=>`'${s.replaceAll("'","''")}'`;
  for(const name of ['cargo','rustc'])await writeFile(join(bin,`${name}.cmd`),'@echo off\necho unexpected>"%HOME%\\compiler-called"\nexit /b 99\n');
  const script=`$ErrorActionPreference='Stop'\nfunction Invoke-WebRequest { param([switch]$UseBasicParsing,[string]$Uri,[string]$OutFile) if($Uri -eq ${q(release+'/'+asset)}) { Copy-Item -LiteralPath ${q(archive)} -Destination $OutFile } elseif($Uri -eq ${q(release+'/'+asset+'.sha256')}) { Copy-Item -LiteralPath ${q(checksum)} -Destination $OutFile } else { Microsoft.PowerShell.Utility\\Invoke-WebRequest @PSBoundParameters } }\n$before=$env:PATH\nGet-Content -Raw -LiteralPath ${q(join(root,'install.ps1'))} | Invoke-Expression\nif($env:PATH -ne $before){throw 'Caller PATH was not restored'}\n`;
  await writeFile(join(fixture,'run.ps1'),script);
  execFileSync(join(env.SYSTEMROOT,'System32/WindowsPowerShell/v1.0/powershell.exe'),['-NoProfile','-NonInteractive','-Command',`Get-Content -Raw -LiteralPath ${q(join(fixture,'run.ps1'))} | Invoke-Expression`],{env,stdio:'inherit',timeout:300000});
}else{
  const curl=execFileSync('/bin/sh',['-c','command -v curl'],{encoding:'utf8'}).trim();
  const wrapper=`#!${process.execPath}\nimport fs from 'node:fs';import cp from 'node:child_process';const a=process.argv.slice(2),out=a[a.indexOf('--output')+1];if(a.includes(${JSON.stringify(release+'/'+asset)}))fs.copyFileSync(${JSON.stringify(archive)},out);else if(a.includes(${JSON.stringify(release+'/'+asset+'.sha256')}))fs.copyFileSync(${JSON.stringify(checksum)},out);else cp.execFileSync(${JSON.stringify(curl)},a,{stdio:'inherit'});\n`;
  await writeFile(join(bin,'curl'),wrapper,{mode:0o755});
  // Hide preinstalled Node/npm until the bootstrap prepends its managed runtime.
  // The curl fixture uses an absolute shebang, so its transport remains usable.
  if(bootstrapNode)for(const name of ['node','npm'])await writeFile(join(bin,name),'#!/bin/sh\nexit 127\n',{mode:0o755});
  for(const name of ['cargo','rustc'])await writeFile(join(bin,name),'#!/bin/sh\nprintf unexpected > "$HOME/compiler-called"\nexit 99\n',{mode:0o755});
  execFileSync('/bin/bash',[join(root,'install.sh')],{env,stdio:'inherit',timeout:300000});
}
const managedNode=join(env.ARC_TOOLS_HOME,`node-v22.23.4-${process.platform==='win32'?'win':process.platform}-${process.arch}`,process.platform==='win32'?'node.exe':'bin/node');
if(bootstrapNode)assert.equal(execFileSync(managedNode,['--version'],{env,encoding:'utf8'}).trim(),'v22.23.4');
else await assert.rejects(access(env.ARC_TOOLS_HOME));
await assert.rejects(access(join(home,'compiler-called')));
const config=JSON.parse(await readFile(join(home,'.config/opencode/opencode.jsonc'),'utf8'));
const binary=config.mcp.arc.command[0];
assert.equal(execFileSync(binary,['--version'],{env,encoding:'utf8'}).trim(),`arc-mcp ${version}`);
console.log(JSON.stringify({passed:true,platform,arch:process.arch,version,bootstrapExecuted:true,checksumVerified:true,noCompilerInvoked:true,installedConfiguration:true,node:bootstrapNode?'official managed download':'existing runtime retained',transport:'local release-download fixture; Node download is real'}));
