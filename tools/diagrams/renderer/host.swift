// Guild-owned WebKit helper. AppKit remains on this executable's main thread.
import AppKit
import WebKit

let root = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
let requestFinished = DispatchSemaphore(value: 0)
final class Host: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
  var view: WKWebView!
  var ready = false
  var pending: [String: Any]?
  var busy = false
  func reply(_ body: [String: Any]) {
    let encoded = try? JSONSerialization.data(withJSONObject: body)
    let data: Data
    if let encoded = encoded, encoded.count < 24 * 1024 * 1024 {
      data = encoded
    } else {
      data = Data("{\"error\":\"Diagram reply exceeds transport budget\"}".utf8)
    }
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data([10]))
    busy = false
    requestFinished.signal()
  }
  func userContentController(
    _ controller: WKUserContentController, didReceive message: WKScriptMessage
  ) {
    guard busy, let data = message.body as? [String: Any] else { return }
    if let svg = data["svg"] as? String, let width = data["width"] as? Double,
      let height = data["height"] as? Double
    {
      guard width.isFinite, height.isFinite, width > 0, height > 0, width <= 4096, height <= 4096,
        ceil(width) * ceil(height) <= 1024 * 1024
      else {
        reply(["error": "Diagram exceeds image budget"])
        return
      }
      view.setFrameSize(NSSize(width: ceil(width), height: ceil(height)))
      // A snapshot keeps WebKit's full SVG text/CSS/foreignObject rendering.
      // It is returned as native pixels, never a WebView overlay in the editor.
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
        self.view.takeSnapshot(with: nil) { image, error in
          guard let image = image, let tiff = image.tiffRepresentation,
            let rep = NSBitmapImageRep(data: tiff),
            let png = rep.representation(using: .png, properties: [:]),
            png.count <= 12 * 1024 * 1024
          else {
            self.reply(["error": "Cannot rasterize diagram"])
            return
          }
          // Retina backing may exceed the logical budget; reject before transport.
          guard rep.pixelsWide * rep.pixelsHigh <= 4 * 1024 * 1024 else {
            self.reply(["error": "Diagram exceeds decoded image budget"])
            return
          }
          self.reply([
            "svg": svg, "png": png.base64EncodedString(), "width": rep.pixelsWide,
            "height": rep.pixelsHigh, "logicalWidth": ceil(width), "logicalHeight": ceil(height),
          ])
        }
      }
    } else {
      reply(data)
    }
  }
  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    ready = true
    if let request = pending {
      pending = nil
      render(request)
    }
  }
  func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
    reply(["error": "WebKit renderer stopped"])
    app.terminate(nil)
  }
  func webView(
    _ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
    decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
  ) {
    decisionHandler(
      !ready && action.request.url?.absoluteString == "about:blank" ? .allow : .cancel)
  }
  func start() {
    let config = WKWebViewConfiguration()
    config.websiteDataStore = .nonPersistent()
    config.userContentController.add(self, name: "result")
    WKContentRuleListStore.default().compileContentRuleList(
      forIdentifier: "guild-diagrams-offline-v1",
      encodedContentRuleList:
        "[{\"trigger\":{\"url-filter\":\"^https?://\"},\"action\":{\"type\":\"block\"}}]"
    ) { rules, error in
      guard let rules = rules else {
        self.reply(["error": "Cannot restrict WebKit resources"])
        app.terminate(nil)
        return
      }
      config.userContentController.add(rules)
      self.view = WKWebView(
        frame: NSRect(x: 0, y: 0, width: 1024, height: 768), configuration: config)
      self.view.navigationDelegate = self
      self.view.loadHTMLString(
        "<meta charset=utf-8><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src blob: 'unsafe-inline' 'unsafe-eval'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'; frame-src 'none'; worker-src 'none'; font-src 'none'\"><style>html,body{margin:0;padding:0;background:white}</style><div id=out></div>",
        baseURL: nil)
    }
  }
  func render(_ request: [String: Any]) {
    guard !busy else {
      reply(["error": "Renderer is busy"])
      return
    }
    guard ready else {
      pending = request
      return
    }
    guard let engine = request["engine"] as? String, let source = request["source"] as? String,
      source.utf8.count <= 262144
    else {
      reply(["error": "Invalid diagram request"])
      return
    }
    busy = true
    let directory: String
    let files: [String]
    switch engine {
    case "mermaid":
      directory = "mermaid-11.12.0"
      files = ["mermaid.min.js"]
    case "plantuml":
      directory = "plantuml-1.2026.9"
      files = ["viz-global.js", "plantuml.js"]
    default:
      reply(["error": "Unknown diagram engine"])
      return
    }
    do {
      var codes: [String] = []
      for file in files {
        let data = try Data(
          contentsOf: root.appendingPathComponent(directory).appendingPathComponent(file))
        guard data.count < 5 * 1024 * 1024 else {
          throw NSError(domain: "Engine file exceeds size limit", code: 1)
        }
        codes.append(data.base64EncodedString())
      }
      let payload = try JSONSerialization.data(withJSONObject: [
        "engine": engine, "source": source, "codes": codes,
      ])
      let encoded = payload.base64EncodedString()
      let js = """
        void (async()=>{try{
        const decode=b=>new TextDecoder().decode(Uint8Array.from(atob(b),c=>c.charCodeAt(0)));
        const req=JSON.parse(decode('\(encoded)'));
        if(!window.guildEngine || window.guildEngine!==req.engine){
          if(req.engine==='mermaid') { await new Promise((ok,no)=>{const script=document.createElement('script');script.src=URL.createObjectURL(new Blob([decode(req.codes[0])],{type:'text/javascript'}));script.onload=()=>{URL.revokeObjectURL(script.src);ok()};script.onerror=no;document.head.append(script)}); mermaid.initialize({startOnLoad:false,securityLevel:'strict',maxTextSize:262144,maxEdges:1000,htmlLabels:true,flowchart:{htmlLabels:true},secure:['securityLevel','maxEdges','maxTextSize','htmlLabels','flowchart','themeCSS']}); }
          else { await new Promise((ok,no)=>{const script=document.createElement('script');script.src=URL.createObjectURL(new Blob([decode(req.codes[0])],{type:'text/javascript'}));script.onload=()=>{URL.revokeObjectURL(script.src);ok()};script.onerror=no;document.head.append(script)}); const url=URL.createObjectURL(new Blob([decode(req.codes[1])],{type:'text/javascript'}));window.guildPlant=await import(url);URL.revokeObjectURL(url); }
          window.guildEngine=req.engine;
        }
        window.PLANTUML_FILE_LOADER=()=>{throw Error('External files are disabled')};
        window.PLANTUML_STDLIB_LOADER=()=>{throw Error('External libraries are disabled')};
        document.querySelector('#out')?.replaceChildren();
        const svg=req.engine==='mermaid'?(await mermaid.render('guild-diagram',req.source)).svg:await new Promise((ok,no)=>guildPlant.renderToString(req.source.split('\\n'),ok,no,{maxSvgSize:4096}));
        const doc=new DOMParser().parseFromString(svg,'image/svg+xml');
        if(doc.querySelector('parsererror'))throw Error('Invalid diagram output');
        const lines=Array.from(doc.querySelectorAll('text')).map(e=>e.textContent);
        const texts=lines.join(' ');
        if(req.engine==='plantuml' && lines[0]?.startsWith('PlantUML version ') && lines.some(t=>t.startsWith('[From textarea (line ')))throw Error(texts.slice(-2000));
        if(doc.querySelector('script,iframe,image'))throw Error('Diagram uses unsupported active or external resources');
        for(const el of doc.querySelectorAll('*'))for(const a of Array.from(el.attributes))if(/^on/i.test(a.name)||(/href$/i.test(a.name)&&!a.value.startsWith('#'))){el.removeAttribute(a.name);}
        for(const el of doc.querySelectorAll('style'))if(/@import|url\\s*\\(/i.test(el.textContent))throw Error('External diagram styling is disabled');
        if(svg.length>8388608)throw Error('Diagram output exceeds size limit');
        const root=doc.documentElement;
        const box=(root.getAttribute('viewBox')||'').trim().split(/[\\s,]+/).map(Number);
        if(box.length!==4||!box.every(Number.isFinite)||box[2]<=0||box[3]<=0)throw Error('Invalid natural diagram dimensions');
        const width=Math.ceil(box[2]),height=Math.ceil(box[3]);
        if(width>4096||height>4096||width*height>1048576)throw Error('Diagram exceeds image budget');
        root.setAttribute('width',String(width));root.setAttribute('height',String(height));
        root.style.setProperty('max-width','none');
        const serialized=new XMLSerializer().serializeToString(doc);
        const out=document.querySelector('#out');out.innerHTML=serialized;
        window.webkit.messageHandlers.result.postMessage({svg:serialized,width,height});
        }catch(e){window.webkit.messageHandlers.result.postMessage({error:String(e).slice(0,16384)})}})()
        """
      view.evaluateJavaScript(js) { _, error in
        if let error = error, self.busy { self.reply(["error": error.localizedDescription]) }
      }
    } catch { reply(["error": "Local engine package cannot be read"]) }
  }
}
let host = Host()
host.start()
DispatchQueue.global().async {
  var pending = Data()
  var buffer = [UInt8](repeating: 0, count: 4096)
  input: while true {
    let count = read(STDIN_FILENO, &buffer, buffer.count)
    if count <= 0 { break }
    for byte in buffer.prefix(count) {
      if byte == 10 {
        guard let request = (try? JSONSerialization.jsonObject(with: pending)) as? [String: Any]
        else { break input }
        pending.removeAll(keepingCapacity: true)
        DispatchQueue.main.async { host.render(request) }
        requestFinished.wait()
      } else {
        if pending.count >= 6 * 262144 + 4096 { break input }
        pending.append(byte)
      }
    }
  }
  DispatchQueue.main.async { app.terminate(nil) }
}
app.run()
