// GitHub's pwsh environment can point Windows PowerShell 5.1 at incompatible
// PowerShell 7 modules. Use the matching built-in modules for archive operations.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {join} from 'node:path';
export function windowsPowerShell(script, options={}) {
  const inherited=options.env ?? process.env;
  const env=Object.fromEntries(Object.entries(inherited).filter(([key])=>key.toLowerCase()!=='psmodulepath'));
  const systemRoot=Object.entries(env).find(([key])=>key.toLowerCase()==='systemroot')?.[1];
  assert.ok(systemRoot,'SYSTEMROOT is required for Windows PowerShell');
  const directory=join(systemRoot,'System32/WindowsPowerShell/v1.0');
  env.PSModulePath=join(directory,'Modules');
  return execFileSync(join(directory,'powershell.exe'),['-NoProfile','-NonInteractive','-Command',`$ErrorActionPreference='Stop'; ${script}`],{...options,env});
}
