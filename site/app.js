const commands={private:'gh api repos/Pumkin4534/arc-mcp/contents/install.sh --jq .content | base64 --decode | ARC_PRIVATE=1 bash',public:'curl -fsSL https://raw.githubusercontent.com/Pumkin4534/arc-mcp/v0.1.1/install.sh | bash'};
const command=document.querySelector('#install-command');
const status=document.querySelector('#copy-status');
document.querySelectorAll('input[name="access"]').forEach(input=>input.addEventListener('change',()=>{
  command.textContent=commands[input.value];status.textContent='';
  document.querySelector('#access-note').textContent=input.value==='public'?'Prepared for the public release. This anonymous URL will not work while the repository is private. Owner approval is still required.':'Requires repository access and an authenticated GitHub CLI. No token is placed in the command.';
}));
document.querySelector('#copy').addEventListener('click',async()=>{
  try{await navigator.clipboard.writeText(command.textContent);status.textContent='Command copied.';}
  catch{status.textContent='Copy is unavailable here. Select the command above and copy it manually.';command.parentElement.focus();}
});
let reviewed=false;
document.querySelector('#demo').addEventListener('click',event=>{
  reviewed=!reviewed;
  document.querySelector('#example-state').textContent=reviewed?'Reviewed':'Pending';
  document.querySelector('#example-state').classList.toggle('pending',!reviewed);
  document.querySelector('#example-message').textContent=reviewed?'Both example judgments pass. Current evidence matches.':'Waiting for two reviewer judgments.';
  event.currentTarget.textContent=reviewed?'Reset the example ↺':'Show the reviewed state →';
});
