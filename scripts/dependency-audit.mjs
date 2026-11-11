// Public package names/versions only are sent to OSV. No code or local paths.
import { readFile } from 'node:fs/promises';
const lock=await readFile(new URL('../Cargo.lock',import.meta.url),'utf8');
const packages=lock.split('[[package]]').slice(1).filter(block=>/source = "registry\+/.test(block)).map(block=>({package:{name:block.match(/\nname = "([^"]+)"/)[1],ecosystem:'crates.io'},version:block.match(/\nversion = "([^"]+)"/)[1]}));
const response=await fetch('https://api.osv.dev/v1/querybatch',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({queries:packages}),signal:AbortSignal.timeout(60000)});
if(!response.ok)throw new Error(`OSV unavailable (${response.status}); audit incomplete`);
const data=await response.json();
if(!Array.isArray(data.results)||data.results.length!==packages.length)throw new Error('Incomplete OSV response');
const findings=data.results.flatMap((result,index)=>(result.vulns??[]).map(vuln=>({package:packages[index].package.name,version:packages[index].version,id:vuln.id})));
console.log(JSON.stringify({scanner:'OSV',packages:packages.length,findings},null,2));
if(findings.length)process.exitCode=1;
