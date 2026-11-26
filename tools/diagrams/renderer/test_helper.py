"""Real optional artifact/engine test, using catalog-verified prepared archives and temporary state."""
from pathlib import Path
import base64, hashlib, json, os, selectors, signal, socket, subprocess, shutil, tempfile, unittest, re, math, xml.etree.ElementTree as ET

ROOT=Path(__file__).resolve().parents[3]
ASSETS=ROOT/'crates/desktop/assets/diagrams'
CATALOG=json.loads((ASSETS/'PROVENANCE.json').read_text())['engines']
RESULTS=ROOT/'target/diagram-runtime-results'
RESULTS.mkdir(parents=True,exist_ok=True)

class HelperTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Explicit development-only native PNG inspection. Uses the installed
        # toolchain, never downloads one and never alters the helper artifact.
        cls.probe=RESULTS/'pixel-probe'
        subprocess.run(['xcrun','--no-cache','swiftc','-O',str(Path(__file__).with_name('pixel_probe.swift')),
                        '-framework','AppKit','-o',str(cls.probe)],check=True)
    def pixels(self,result,regions=(),**control):
        request=dict(png=result['png'],regions=regions,**control)
        output=subprocess.run([str(self.probe)],input=json.dumps(request).encode(),stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True,timeout=15)
        self.assertEqual(output.stderr,b'','native pixel probe must not report codec/color errors')
        return json.loads(output.stdout)
    def label_regions(self,result):
        # For this real flowchart fixture, Mermaid places each foreignObject
        # under a node translation and a label translation. Inspect its interior,
        # excluding node borders and connecting arrows, in physical PNG pixels.
        ns='{http://www.w3.org/2000/svg}'
        root=ET.fromstring(result['svg'])
        box=list(map(float,root.attrib['viewBox'].split()))
        def translate(element):
            value=re.fullmatch(r'translate\(([\d.eE+\-]+)[ ,]+([\d.eE+\-]+)\)',element.attrib['transform'])
            self.assertIsNotNone(value,'fixture uses only explicit label/node translations')
            return tuple(map(float,value.groups()))
        regions=[]
        for node in root.iter(ns+'g'):
            if 'node' not in node.attrib.get('class','').split():continue
            label=next(el for el in node.iter(ns+'g') if 'label' in el.attrib.get('class','').split())
            foreign=label.find(ns+'foreignObject')
            self.assertIsNotNone(foreign)
            tx,ty=translate(node);lx,ly=translate(label)
            sx=result['width']/result['logicalWidth'];sy=result['height']/result['logicalHeight']
            x=math.ceil((tx+lx-box[0]+1)*sx);y=math.ceil((ty+ly-box[1]+1)*sy)
            right=math.floor((tx+lx-box[0]+float(foreign.attrib['width'])-1)*sx)
            bottom=math.floor((ty+ly-box[1]+float(foreign.attrib['height'])-1)*sy)
            regions.append([x,y,right-x,bottom-y])
        self.assertEqual(len(regions),2,'both Cyrillic foreignObject labels must be checked')
        return regions
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='guild-diagram-test-')
        self.addCleanup(self.temp.cleanup)
        self.state=Path(self.temp.name)
        app=os.environ.get('GUILD_DIAGRAM_TEST_APP')
        origin=Path(app)/'Contents/Resources/diagrams' if app else ROOT/'target/diagram-runtime'
        executable=Path(app)/'Contents/MacOS/guild-diagram-renderer' if app else origin/'guild-diagram-renderer'
        for package in CATALOG:
            folder=package['id']+'-'+package['version']
            for item in package['files']:
                data=(origin/folder/item['name']).read_bytes()
                self.assertEqual(len(data),item['bytes'])
                self.assertEqual(hashlib.sha256(data).hexdigest(),item['sha256'])
            shutil.copytree(origin/folder,self.state/folder)
        self.helper=self.state/'guild-diagram-renderer'
        shutil.copy2(executable,self.helper)
        self.assertEqual(hashlib.sha256(self.helper.read_bytes()).hexdigest(),(origin/'helper.sha256').read_text().strip())
        self.process=subprocess.Popen([str(self.helper),str(self.state)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,start_new_session=True)
        self.addCleanup(self.stop)
    def stop(self):
        if self.process.poll() is None:
            os.killpg(self.process.pid,signal.SIGKILL)
        self.process.wait(timeout=5)
        self.process.stdin.close();self.process.stdout.close()
    def render(self,engine,source):
        self.process.stdin.write(json.dumps(dict(engine=engine,source=source),ensure_ascii=False).encode()+b'\n');self.process.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout,selectors.EVENT_READ)
            self.assertTrue(selector.select(15),'helper render deadline')
        line=self.process.stdout.readline(24*1024*1024)
        self.assertTrue(line,'helper exited before result')
        return json.loads(line)
    def check_image(self,result,regions=()):
        self.assertNotIn('error',result)
        png=base64.b64decode(result['png'])
        self.assertEqual(png[:8],b'\x89PNG\r\n\x1a\n')
        width=int.from_bytes(png[16:20],'big');height=int.from_bytes(png[20:24],'big')
        self.assertGreater(width,10);self.assertGreater(height,10)
        self.assertLessEqual(width*height,4*1024*1024)
        pixels=self.pixels(result,regions)
        self.assertEqual((pixels['width'],pixels['height']),(width,height))
        self.assertGreater(pixels['whole']['ink'],50,'PNG must contain a painted graph')
        self.assertGreater(pixels['whole']['dark'],20,'PNG must contain painted dark content')
        for label in pixels['regions']:
            self.assertGreater(label['dark'],20,'PNG foreignObject label interior must contain painted text')
        return png
    def test_cyrillic_errors_and_recovery(self):
        for engine,source,invalid in [('mermaid','flowchart LR\n A["Исходный текст"] --> B["Предпросмотр"]','flowchart LR\n A['),('plantuml','@startuml\nAlice -> Bob: Привет\n@enduml','@startuml\ninvalid diagram here\n@enduml')]:
            result=self.render(engine,source);png=self.check_image(result)
            (RESULTS/f'{engine}.png').write_bytes(png)
            (RESULTS/f'{engine}-geometry.json').write_text(json.dumps({k:v for k,v in result.items() if k not in ['png','svg']}))
            self.assertIn('т',result['svg'])
            # A valid white PNG has the same dimensions/signature and SVG text,
            # but must fail the actual pixel assertion for both engines.
            blank=self.pixels(result,blank=True)
            with self.assertRaisesRegex(AssertionError,'painted'):
                self.check_image(dict(result,png=blank['png']))
            (RESULTS/f'code02-{engine}-blank-control.png').write_bytes(base64.b64decode(blank['png']))
            if engine=='mermaid':
                self.assertIn('foreignObject',result['svg'])
                regions=self.label_regions(result)
                self.check_image(result,regions)
                missing=self.pixels(result,regions,eraseLabels=True)
                self.assertGreater(missing['whole']['dark'],20,'negative control retains arrows/graph pixels')
                with self.assertRaisesRegex(AssertionError,'label interior'):
                    self.check_image(dict(result,png=missing['png']),regions)
                (RESULTS/'code02-mermaid-no-labels-control.png').write_bytes(base64.b64decode(missing['png']))
                (RESULTS/'code02-label-pixels.json').write_text(json.dumps(dict(regions=regions,actual=self.pixels(result,regions),erased={k:v for k,v in missing.items() if k!='png'}),indent=2))
            self.assertIn('error',self.render(engine,invalid))
            self.check_image(self.render(engine,source))
        self.check_image(self.render('plantuml','@startuml\nAlice -> Bob: Syntax Error / Cannot include / Error line are valid labels\n@enduml'))
        self.check_image(self.render('plantuml','@startuml\nclass Guild\nclass Diagram\nGuild --> Diagram\n@enduml'))
        self.check_image(self.render('plantuml','@startuml\n[Editor] --> [Preview]\n@enduml'))
    def test_natural_geometry_and_logical_size(self):
        for source in ['flowchart TD\n A-->B','sequenceDiagram\n Alice->>Bob: Привет']:
            result=self.render('mermaid',source);png=self.check_image(result)
            import xml.etree.ElementTree as ET, math
            svg=ET.fromstring(result['svg']);box=[float(x) for x in svg.attrib['viewBox'].replace(',',' ').split()]
            self.assertEqual(result['logicalWidth'],math.ceil(box[2]))
            self.assertEqual(result['logicalHeight'],math.ceil(box[3]))
            self.assertLess(result['logicalWidth'],700)
            self.assertLess(result['logicalHeight'],700)
            self.assertEqual(int.from_bytes(png[16:20],'big'),result['width'])
    def test_wire_expansion_and_decoded_limit(self):
        source="@startuml\n'"+('"\\'*110000)+"\nAlice -> Bob: valid\n@enduml"
        self.assertLess(len(source.encode()),262144)
        self.assertGreater(len(json.dumps(dict(engine='plantuml',source=source)).encode()),400000)
        self.check_image(self.render('plantuml',source))
        self.assertIn('error',self.render('plantuml',' '*262145))
    def test_no_network_or_local_include(self):
        with socket.socket() as listener:
            listener.bind(('127.0.0.1',0));listener.listen();listener.settimeout(.2)
            port=listener.getsockname()[1]
            for include in [f'!includeurl http://127.0.0.1:{port}/secret','!include /etc/passwd']:
                self.assertIn('error',self.render('plantuml',f'@startuml\n{include}\nAlice -> Bob\n@enduml'))
            self.render('mermaid',f'''flowchart LR\n A["<img src='http://127.0.0.1:{port}/secret'/>Привет"]''')
            with self.assertRaises(socket.timeout):listener.accept()
    def test_eof_and_kill_fresh_recovery(self):
        self.check_image(self.render('mermaid','flowchart LR\n A-->B'))
        self.process.stdin.close();self.process.wait(timeout=5)
        self.assertEqual(self.process.returncode,0)
        self.process.stdout.close()
        self.process=subprocess.Popen([str(self.helper),str(self.state)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,start_new_session=True)
        self.check_image(self.render('mermaid','flowchart LR\n A-->B'))
        self.process.stdin.write(json.dumps(dict(engine='mermaid',source='flowchart LR\n'+'\n'.join(f'A{i}-->A{i+1}' for i in range(900)))).encode()+b'\n');self.process.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout,selectors.EVENT_READ)
            self.assertFalse(selector.select(.02),'large render still in flight at cancellation')
        os.killpg(self.process.pid,signal.SIGKILL);self.process.wait(timeout=5)
        self.process.stdin.close();self.process.stdout.close()
        self.process=subprocess.Popen([str(self.helper),str(self.state)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,start_new_session=True)
        self.check_image(self.render('plantuml','@startuml\nAlice -> Bob: recovered\n@enduml'))

if __name__=='__main__':unittest.main(verbosity=2)
