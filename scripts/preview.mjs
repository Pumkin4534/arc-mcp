import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve,extname,sep,dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../site');
const types={'.html':'text/html; charset=utf-8','.css':'text/css; charset=utf-8','.js':'text/javascript; charset=utf-8','.svg':'image/svg+xml'};
createServer(async(req,res)=>{
  try{
    const path=decodeURIComponent(new URL(req.url,'http://localhost').pathname);
    const file=resolve(root,`.${path==='/'?'/index.html':path}`);
    if(!file.startsWith(root+sep))throw new Error('outside root');
    const bytes=await readFile(file);res.writeHead(200,{'Content-Type':types[extname(file)]??'application/octet-stream','X-Content-Type-Options':'nosniff','Cache-Control':'no-store'});res.end(bytes);
  }catch{res.writeHead(404,{'Content-Type':'text/plain'});res.end('Not found');}
}).listen(Number(process.env.PORT??4173),'127.0.0.1',()=>console.log('Private local preview: http://127.0.0.1:4173'));
