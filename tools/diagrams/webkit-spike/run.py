"""Run the native proof with a local network canary; no rendering service."""
from pathlib import Path
import subprocess,socket,time,json,tarfile
root=Path(__file__).resolve().parents[3];dest=root/'target/webkit-spike'
listener=socket.socket();listener.bind(('127.0.0.1',19876));listener.listen();listener.setblocking(False)
connections=0;samples=[]
with (dest/'verified-results.jsonl').open('w') as out,(dest/'verified-time.txt').open('w') as err:
 p=subprocess.Popen(['/usr/bin/time','-l',str(dest/'host'),str(dest/'run')],stdout=out,stderr=err)
 while p.poll() is None:
  try:
   conn,_=listener.accept();connections+=1;conn.close()
  except BlockingIOError:pass
  rows=subprocess.check_output(['ps','-axo','pid,ppid,rss,comm'],text=True)
  samples.extend(line.strip() for line in rows.splitlines() if 'webkit-spike/host' in line or 'com.apple.WebKit.WebContent' in line)
  time.sleep(.03)
listener.close()
(dest/'process-samples.txt').write_text('\n'.join(sorted(set(samples)))+'\n')
print('canary_connections',connections,'exit',p.returncode)
assert connections==0 and p.returncode==0
for name,files in [('mermaid',[(root/'tools/diagrams/node_modules/mermaid/dist/mermaid.min.js','mermaid.min.js'),(root/'tools/diagrams/node_modules/mermaid/LICENSE','LICENSE')]),('plantuml',[(dest/'core/package'/x,x) for x in ['plantuml.js','viz-global.js','LICENSE']])]:
 archive=dest/(name+'-minimal.tgz')
 with tarfile.open(archive,'w:gz') as tar:
  for path,label in files:tar.add(path,arcname=label)
 print(name,'download',archive.stat().st_size,'unpacked',sum(p.stat().st_size for p,_ in files))
