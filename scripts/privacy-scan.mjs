// Scans distributable source, not a developer's state, caches or test artifacts.
// Extra private identifiers may be supplied via ARC_PRIVACY_DENY (comma-separated).
import { readdir,readFile,lstat } from 'node:fs/promises';
import { dirname,join,resolve,relative } from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const ignored=new Set(['.git','target','node_modules','dist']);
const rules=[
  ['personal home path',/(?:\/Users\/|\/home\/)[a-zA-Z0-9._-]+/],
  ['private key',/-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/],
  ['GitHub credential',/\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{50,})\b/],
  ['cloud credential',/\bAKIA[0-9A-Z]{16}\b/],
  ['embedded build directory',/env!\("CARGO_MANIFEST_DIR"\)/],
  ['local session identity',/\b(?:ses|msg|prt)_[a-z0-9]{20,}\b/i],
];
const forbidden=[homedir(),...(process.env.ARC_PRIVACY_DENY?.split(',').filter(Boolean)??[])];
let files=0; const findings=[];
async function scan(dir){
  for(const entry of await readdir(dir,{withFileTypes:true})){
    if(ignored.has(entry.name)) continue;
    const path=join(dir,entry.name); const name=relative(root,path);
    if(entry.isSymbolicLink()) { findings.push({file:name,rule:'symlink in distribution'}); continue; }
    if(entry.isDirectory()){await scan(path);continue;}
    if(/(?:^\.env(?:\.|$)|\.log$|\.bak$|^AGENT_HANDOFF|^agent_.*report|^state\.json$)/.test(entry.name)) findings.push({file:name,rule:'private/generated file'});
    const text=(await readFile(path)).toString('utf8'); files++;
    for(const [rule,pattern]of rules)if(pattern.test(text))findings.push({file:name,rule});
    if(forbidden.some(value=>text.includes(value)))findings.push({file:name,rule:'private identifier'});
  }
}
await scan(root);
if(process.argv.includes('--binary')){
  const bytes=await readFile(join(root,'target/release/arc-mcp'));
  for(const token of forbidden)if(bytes.includes(Buffer.from(token)))findings.push({file:'release binary',rule:'actual home path or private identifier'});
  // Generic path-remapping prefixes are not a developer's actual home path.
  if(/(?:\/Users\/|\/home\/)[A-Za-z0-9_-]+\//.test(bytes.toString('latin1')))findings.push({file:'release binary',rule:'identifying absolute home path'});
}
console.log(JSON.stringify({files,findings},null,2));
if(findings.length)process.exitCode=1;
