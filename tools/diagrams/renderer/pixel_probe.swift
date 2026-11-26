// Development test tool only: AppKit decodes PNGs; no custom PNG decoder.
import AppKit

let input = try JSONSerialization.jsonObject(with: FileHandle.standardInput.readDataToEndOfFile()) as! [String: Any]
let png = Data(base64Encoded: input["png"] as! String)!
let decoded = NSBitmapImageRep(data: png)!
let width = decoded.pixelsWide, height = decoded.pixelsHigh
precondition(width > 0 && height > 0 && width * height <= 4 * 1024 * 1024)
// Normalize into a known editable RGB bitmap. Decoded PNG color models need
// not support NSBitmapImageRep.setColor (e.g. the WebKit snapshot's profile).
let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
  bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
  colorSpaceName: .deviceRGB, bytesPerRow: width * 4, bitsPerPixel: 32)!
let context = NSGraphicsContext(bitmapImageRep: bitmap)!
context.cgContext.draw(decoded.cgImage!, in: CGRect(x: 0, y: 0, width: width, height: height))
let white = NSColor(deviceRed: 1, green: 1, blue: 1, alpha: 1)
let regions = input["regions"] as? [[Int]] ?? []
func pixels(_ region: [Int], _ visit: (Int, Int) -> Void) {
  precondition(region.count == 4 && region[2] > 0 && region[3] > 0)
  precondition(region[0] >= 0 && region[1] >= 0 && region[0] + region[2] <= width && region[1] + region[3] <= height)
  for y in region[1]..<(region[1] + region[3]) {
    for x in region[0]..<(region[0] + region[2]) { visit(x, y) }
  }
}
let whole = [0, 0, width, height]
// Negative controls retain valid PNG decode/dimensions, and optionally retain
// borders/arrows while erasing only the foreignObject label interiors.
if input["blank"] as? Bool == true {
  pixels(whole) { x, y in bitmap.setColor(white, atX: x, y: y) }
} else if input["eraseLabels"] as? Bool == true {
  for region in regions { pixels(region) { x, y in bitmap.setColor(white, atX: x, y: y) } }
}
func counts(_ region: [Int]) -> [String: Int] {
  var ink = 0, dark = 0
  pixels(region) { x, y in
    let color = bitmap.colorAt(x: x, y: y)!.usingColorSpace(.deviceRGB)!
    if color.alphaComponent > 0.5 {
      let low = min(color.redComponent, color.greenComponent, color.blueComponent)
      let high = max(color.redComponent, color.greenComponent, color.blueComponent)
      if low < 0.9 { ink += 1 }
      if high < 0.5 { dark += 1 }
    }
  }
  return ["ink": ink, "dark": dark]
}
var output: [String: Any] = ["width": width, "height": height, "whole": counts(whole), "regions": regions.map(counts)]
if input["blank"] as? Bool == true || input["eraseLabels"] as? Bool == true {
  output["png"] = bitmap.representation(using: .png, properties: [:])!.base64EncodedString()
}
FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: output, options: [.sortedKeys]))
