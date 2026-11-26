from pathlib import Path
import shutil,json,gzip,hashlib,base64
root=Path(__file__).resolve().parents[3];dest=root/'target/webkit-spike/run';dest.mkdir(parents=True,exist_ok=True)
assets={'mermaid.min.js':root/'tools/diagrams/node_modules/mermaid/dist/mermaid.min.js','plantuml.js':root/'target/webkit-spike/core/package/plantuml.js','viz-global.js':root/'target/webkit-spike/core/package/viz-global.js'}
for name,path in assets.items():
 shutil.copy2(path,dest/name)
 data=path.read_bytes();print(name,len(data),len(gzip.compress(data,mtime=0)),hashlib.sha256(data).hexdigest())
html='''<meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' blob: 'unsafe-inline' 'unsafe-eval'; style-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'; worker-src 'none'; frame-src 'none'; font-src 'none'">
<div id="out"></div><script src="mermaid.min.js"></script><script src="viz-global.js"></script><script type="module">
const {renderToString} = await import(URL.createObjectURL(new Blob([Uint8Array.from(atob('ENGINE_BASE64'), c=>c.charCodeAt(0))],{type:'text/javascript'})));
const post=x=>window.webkit.messageHandlers.result.postMessage(x);
const started=performance.now();let displaySvg="";
mermaid.initialize({startOnLoad:false,securityLevel:'strict',flowchart:{htmlLabels:false},maxEdges:1000});
globalThis.PLANTUML_FILE_LOADER=()=>{throw Error('External files disabled')};
globalThis.PLANTUML_STDLIB_LOADER=()=>{throw Error('External libraries disabled')};
const tests=[
['mermaid-cyrillic','m','flowchart LR\\n A["Исходный текст"] --> B["Предпросмотр диаграммы"]'],
['mermaid-error','m','flowchart LR\\n A['],
['mermaid-recovery','m','flowchart LR\\n A-->B'],
['plantuml-sequence','p','@startuml\\nAlice -> Bob: Привет\\n@enduml'],
['plantuml-class','p','@startuml\\nclass Guild\\nclass Diagram\\nGuild --> Diagram\\n@enduml'],
['plantuml-component','p','@startuml\\n[Editor] --> [Preview]\\n@enduml'],
['plantuml-error','p','@startuml\\ninvalid diagram here\\n@enduml'],
['plantuml-recovery','p','@startuml\\nAlice -> Bob: recovered\\n@enduml'],
['plantuml-remote','p','@startuml\\n!includeurl http://127.0.0.1:19876/secret\\nAlice -> Bob\\n@enduml'],
['plantuml-file','p','@startuml\\n!include /etc/passwd\\nAlice -> Bob\\n@enduml']];
for(const [name,engine,source] of tests){const t=performance.now();try{
const svg=engine==='m'?(await mermaid.render('diagram',source)).svg:await new Promise((ok,fail)=>renderToString(source.split('\\n'),ok,fail,{maxSvgSize:4096}));
const bad=/Syntax Error|Cannot include|cannot include|Error line|ErrorLine/.test(svg);
post({name,ms:performance.now()-t,status:bad?'engine-error-svg':'svg',bytes:svg.length,svg});
if(name==='mermaid-cyrillic')displaySvg=svg;
}catch(e){post({name,ms:performance.now()-t,status:'error',error:String(e).slice(0,500)})}}
document.body.innerHTML='<div id=out>'+displaySvg+'</div>';await new Promise(ok=>setTimeout(ok,150));
for (const url of ['http://127.0.0.1:19876/secret','file:///etc/passwd']) {try {await fetch(url);post({name:'host-fetch',url,status:'UNEXPECTED_ALLOWED'});}catch(e){post({name:'host-fetch',url,status:'blocked'});}}
post({done:true,jsMs:performance.now()-started,userAgent:navigator.userAgent});
</script>'''
html=html.replace('ENGINE_BASE64',base64.b64encode(assets['plantuml.js'].read_bytes()).decode())
(dest/'index.html').write_text(html)
