import AppKit
import WebKit
let root = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
final class Host: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
 var view: WKWebView!
 var window: NSWindow!
 let started = Date()
 func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
  guard let data = message.body as? [String:Any] else { return }
  if let svg=data["svg"] as? String, let name=data["name"] as? String {
   try? svg.write(to:root.appendingPathComponent(name+".svg"),atomically:true,encoding:.utf8)
  }
  var info=data;info.removeValue(forKey:"svg")
  if let json=try? JSONSerialization.data(withJSONObject:info,options:[.sortedKeys]),let line=String(data:json,encoding:.utf8){ print(line);fflush(stdout) }
  if data["done"] as? Bool == true {
   print("HOST_TOTAL_MS \(Date().timeIntervalSince(started)*1000)")
   view.takeSnapshot(with:nil){ image,error in
    if let image=image, let tiff=image.tiffRepresentation,let rep=NSBitmapImageRep(data:tiff),let png=rep.representation(using:.png,properties:[:]) {try? png.write(to:root.appendingPathComponent("webkit.png"));print("SNAPSHOT_PASS \(png.count)")}
    else{print("SNAPSHOT_FAIL \(String(describing:error))")}
    app.terminate(nil)
   }
  }
 }
 func webView(_ webView:WKWebView, decidePolicyFor action:WKNavigationAction, decisionHandler:@escaping(WKNavigationActionPolicy)->Void) {
  print("NAV \(String(describing:action.request.url))");fflush(stdout)
  decisionHandler(action.request.url == root.appendingPathComponent("index.html") ? .allow : .cancel)
 }
 func webView(_ webView:WKWebView, didFinish navigation:WKNavigation!) {print("LOADED");fflush(stdout)}
 func webView(_ webView:WKWebView, didFailProvisionalNavigation navigation:WKNavigation!, withError error:Error){print("LOAD_FAIL \(error)")}
 func start() {
  let config=WKWebViewConfiguration();config.websiteDataStore = .nonPersistent()
  config.userContentController.add(self,name:"result")
  config.userContentController.addUserScript(WKUserScript(source:"window.addEventListener('error',e=>window.webkit.messageHandlers.result.postMessage({error:e.message})); window.addEventListener('unhandledrejection',e=>window.webkit.messageHandlers.result.postMessage({error:String(e.reason)}));",injectionTime:.atDocumentStart,forMainFrameOnly:true))
  let rules="""
  [{"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}}]
  """
  WKContentRuleListStore.default().compileContentRuleList(forIdentifier:"guild-diagram-spike-offline",encodedContentRuleList:rules){ list,error in
   guard let list=list else{print("RULE_ERROR \(String(describing:error))");app.terminate(nil);return}
   print("RULES_READY");fflush(stdout)
   config.userContentController.add(list)
   self.view=WKWebView(frame:NSRect(x:0,y:0,width:900,height:700),configuration:config)
   self.view.navigationDelegate=self
   self.window=NSWindow(contentRect:NSRect(x:0,y:0,width:900,height:700),styleMask:[.titled],backing:.buffered,defer:false)
   self.window.title="Guild system WebKit diagram feasibility";self.window.contentView=self.view;self.window.orderBack(nil)
   self.view.loadFileURL(root.appendingPathComponent("index.html"),allowingReadAccessTo:root)
  }
 }
}
let host=Host();host.start()
DispatchQueue.main.asyncAfter(deadline:.now()+45){print("HOST_TIMEOUT");app.terminate(nil)}
app.run()
