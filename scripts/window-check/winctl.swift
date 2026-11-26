// winctl — drive a window from outside, addressed by pid.
//
//   winctl list <name-substring>                 pid, window id, x y w h (top-left origin), title
//   winctl front <pid>                           activate the app and print the frontmost pid
//   winctl click <pid> <x> <y> [right|double]    window-relative points; refuses if <pid> is not frontmost
//   winctl drag <pid> <x1> <y1> <x2> <y2>        press, move in steps, release
//   winctl move <pid> <x> <y>                    pointer only
//   winctl key <pid> <keycode> [cmd] [shift] [alt] [ctrl]
//   winctl tap <pid> shift|cmd|alt|ctrl [count]  lone modifier taps (double Shift)
//   winctl scroll <pid> <x> <y> <dy>             wheel lines, negative is down
//   winctl swipe <pid> <x> <y> <dx> [steps] [--momentum N]  precise horizontal swipe
//   winctl paste <pid> <text>                    pbcopy + cmd-v
//   winctl shot <pid> <file.png>                 screencapture of the window
//
// Every gesture activates the target first, checks the frontmost pid in the
// same process, and refuses otherwise — three Guild windows can sit at one
// place on the display. A click holds the button for 150 ms, because GPUI
// does not reliably see a shorter one.

import Cocoa
import ApplicationServices

func die(_ s: String) -> Never { FileHandle.standardError.write((s + "\n").data(using: .utf8)!); exit(1) }

struct Win { let pid: pid_t; let id: Int; let x, y, w, h: Double; let title: String; let owner: String }

func windows() -> [Win] {
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
    return list.compactMap { d in
        guard let pid = d[kCGWindowOwnerPID as String] as? pid_t,
              let id = d[kCGWindowNumber as String] as? Int,
              let layer = d[kCGWindowLayer as String] as? Int, layer == 0,
              let b = d[kCGWindowBounds as String] as? [String: Double] else { return nil }
        return Win(pid: pid, id: id, x: b["X"] ?? 0, y: b["Y"] ?? 0, w: b["Width"] ?? 0, h: b["Height"] ?? 0,
                   title: d[kCGWindowName as String] as? String ?? "", owner: d[kCGWindowOwnerName as String] as? String ?? "")
    }
}

func window(of pid: pid_t) -> Win {
    guard let w = windows().filter({ $0.pid == pid }).max(by: { $0.w * $0.h < $1.w * $1.h }) else { die("no window for pid \(pid)") }
    return w
}

func activate(_ pid: pid_t) {
    guard let app = NSRunningApplication(processIdentifier: pid) else { die("no process \(pid)") }
    _ = app
    for _ in 0..<20 {
        // NSRunningApplication.activate cannot take the front from another app
        // since macOS 14; System Events can.
        let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        p.arguments = ["-e", "tell application \"System Events\" to set frontmost of (first process whose unix id is \(pid)) to true"]
        try? p.run(); p.waitUntilExit()
        usleep(100_000)
        if NSWorkspace.shared.frontmostApplication?.processIdentifier == pid { return }
    }
    die("pid \(pid) did not become frontmost (front is \(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? -1))")
}

func ensureFront(_ pid: pid_t) {
    if NSWorkspace.shared.frontmostApplication?.processIdentifier != pid {
        activate(pid)
        usleep(250_000)
    }
    guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid else { die("refused: pid \(pid) is not frontmost") }
}

func post(_ e: CGEvent?) { e?.post(tap: .cghidEventTap) }

func point(_ pid: pid_t, _ x: Double, _ y: Double) -> CGPoint {
    let w = window(of: pid)
    return CGPoint(x: w.x + x, y: w.y + y)
}

func mouse(_ type: CGEventType, _ p: CGPoint, _ button: CGMouseButton, clicks: Int64 = 1) {
    let e = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: p, mouseButton: button)
    e?.setIntegerValueField(.mouseEventClickState, value: clicks)
    post(e)
}

let flagsByName: [String: CGEventFlags] = ["cmd": .maskCommand, "shift": .maskShift, "alt": .maskAlternate, "ctrl": .maskControl]
let modifierKeycodes: [String: CGKeyCode] = ["shift": 56, "cmd": 55, "alt": 58, "ctrl": 59]

let args = CommandLine.arguments
guard args.count >= 2 else { die("usage: see the header of winctl.swift") }
switch args[1] {
case "list":
    let needle = args.count > 2 ? args[2].lowercased() : ""
    for w in windows() where needle.isEmpty || w.owner.lowercased().contains(needle) {
        print("\(w.pid) \(w.id) \(Int(w.x)) \(Int(w.y)) \(Int(w.w)) \(Int(w.h)) \(w.owner) | \(w.title)")
    }
case "front":
    let pid = pid_t(args[2])!
    activate(pid)
    print(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? -1)
case "click":
    let pid = pid_t(args[2])!; let p = point(pid, Double(args[3])!, Double(args[4])!)
    let mode = args.count > 5 ? args[5] : "left"
    ensureFront(pid)
    mouse(.mouseMoved, p, .left); usleep(80_000)
    switch mode {
    case "right":
        mouse(.rightMouseDown, p, .right); usleep(150_000); mouse(.rightMouseUp, p, .right)
    case "double":
        mouse(.leftMouseDown, p, .left, clicks: 1); usleep(60_000); mouse(.leftMouseUp, p, .left, clicks: 1)
        usleep(60_000)
        mouse(.leftMouseDown, p, .left, clicks: 2); usleep(60_000); mouse(.leftMouseUp, p, .left, clicks: 2)
    case "triple":
        for n in 1...3 {
            mouse(.leftMouseDown, p, .left, clicks: Int64(n)); usleep(50_000); mouse(.leftMouseUp, p, .left, clicks: Int64(n)); usleep(50_000)
        }
    case "cmd":
        let d = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left); d?.flags = .maskCommand; post(d)
        usleep(150_000)
        let u = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left); u?.flags = .maskCommand; post(u)
    case "shift":
        let d = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left); d?.flags = .maskShift; post(d)
        usleep(150_000)
        let u = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left); u?.flags = .maskShift; post(u)
    default:
        mouse(.leftMouseDown, p, .left); usleep(150_000); mouse(.leftMouseUp, p, .left)
    }
case "drag":
    let pid = pid_t(args[2])!
    let a = point(pid, Double(args[3])!, Double(args[4])!), b = point(pid, Double(args[5])!, Double(args[6])!)
    ensureFront(pid)
    mouse(.mouseMoved, a, .left); usleep(80_000)
    mouse(.leftMouseDown, a, .left); usleep(150_000)
    let steps = 20
    for i in 1...steps {
        let t = Double(i) / Double(steps)
        mouse(.leftMouseDragged, CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t), .left)
        usleep(16_000)
    }
    usleep(100_000)
    mouse(.leftMouseUp, b, .left)
case "dragpath":
    // dragpath <pid> <clicks> x1 y1 x2 y2 [x3 y3 ...]: press at the first point
    // (the press carrying the click count, after count-1 quick clicks), drag
    // through the rest, release at the last.
    let pid = pid_t(args[2])!; let clicks = Int64(args[3])!
    var pts: [CGPoint] = []
    var i = 4
    while i + 1 < args.count { pts.append(point(pid, Double(args[i])!, Double(args[i+1])!)); i += 2 }
    ensureFront(pid)
    let a = pts[0]
    mouse(.mouseMoved, a, .left); usleep(80_000)
    if clicks > 1 { for c in 1..<clicks { mouse(.leftMouseDown, a, .left, clicks: c); usleep(60_000); mouse(.leftMouseUp, a, .left, clicks: c); usleep(60_000) } }
    mouse(.leftMouseDown, a, .left, clicks: clicks); usleep(150_000)
    var prev = a
    for b in pts.dropFirst() {
        for k in 1...20 {
            let t = Double(k) / 20.0
            mouse(.leftMouseDragged, CGPoint(x: prev.x + (b.x - prev.x) * t, y: prev.y + (b.y - prev.y) * t), .left, clicks: clicks)
            usleep(16_000)
        }
        prev = b
        usleep(300_000)
    }
    usleep(100_000)
    mouse(.leftMouseUp, prev, .left, clicks: clicks)
case "move":
    let pid = pid_t(args[2])!; let p = point(pid, Double(args[3])!, Double(args[4])!)
    ensureFront(pid)
    mouse(.mouseMoved, p, .left)
case "key":
    let pid = pid_t(args[2])!; let code = CGKeyCode(args[3])!
    var flags: CGEventFlags = []
    for m in args.dropFirst(4) { guard let f = flagsByName[m] else { die("unknown modifier \(m)") }; flags.insert(f) }
    ensureFront(pid)
    // Modifiers are pressed and released as a keyboard does, with flagsChanged
    // events: a bare keyDown carrying flags leaves GPUI's modifier state stale.
    let mods = args.dropFirst(4).map { String($0) }
    var held: CGEventFlags = []
    for m in mods { held.insert(flagsByName[m]!); let e = CGEvent(keyboardEventSource: nil, virtualKey: modifierKeycodes[m]!, keyDown: true); e?.type = .flagsChanged; e?.flags = held; post(e); usleep(15_000) }
    let down = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true); down?.flags = flags; post(down)
    usleep(40_000)
    let up = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false); up?.flags = flags; post(up)
    for m in mods.reversed() { usleep(15_000); held.remove(flagsByName[m]!); let e = CGEvent(keyboardEventSource: nil, virtualKey: modifierKeycodes[m]!, keyDown: false); e?.type = .flagsChanged; e?.flags = held; post(e) }
case "tap":
    let pid = pid_t(args[2])!; guard let code = modifierKeycodes[args[3]], let flag = flagsByName[args[3]] else { die("tap shift|cmd|alt|ctrl") }
    let count = args.count > 4 ? Int(args[4])! : 1
    ensureFront(pid)
    for _ in 0..<count {
        let d = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true); d?.type = .flagsChanged; d?.flags = flag; post(d)
        usleep(40_000)
        let u = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false); u?.type = .flagsChanged; u?.flags = []; post(u)
        usleep(90_000)
    }
case "scroll":
    let pid = pid_t(args[2])!; let p = point(pid, Double(args[3])!, Double(args[4])!)
    ensureFront(pid)
    mouse(.mouseMoved, p, .left); usleep(60_000)
    post(CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 1, wheel1: Int32(args[5])!, wheel2: 0, wheel3: 0))
case "swipe":
    let pid = pid_t(args[2])!; let p = point(pid, Double(args[3])!, Double(args[4])!)
    let dx = Int32(args[5])!
    var steps = 8
    var momentum = 0
    var i = 6
    if i < args.count && args[i] != "--momentum" { steps = max(1, Int(args[i])!); i += 1 }
    if i + 1 < args.count && args[i] == "--momentum" { momentum = max(0, Int(args[i + 1])!) }
    ensureFront(pid)
    mouse(.mouseMoved, p, .left); usleep(60_000)
    func wheel(_ x: Int32, phase: Int64, momentumPhase: Int64 = 0) {
        let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2,
                            wheel1: 0, wheel2: x, wheel3: 0)
        event?.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
        event?.setIntegerValueField(.scrollWheelEventScrollPhase, value: phase)
        event?.setIntegerValueField(.scrollWheelEventMomentumPhase, value: momentumPhase)
        post(event)
    }
    let part = dx / Int32(steps)
    wheel(part, phase: 1)
    usleep(12_000)
    if steps > 1 {
        for _ in 1..<steps { wheel(part, phase: 2); usleep(12_000) }
    }
    wheel(0, phase: 4)
    if momentum > 0 {
        for n in 0..<momentum {
            let phase: Int64 = n == 0 ? 1 : (n + 1 == momentum ? 4 : 2)
            let tail = part / Int32(n + 2)
            wheel(tail, phase: 0, momentumPhase: phase)
            usleep(16_000)
        }
    }
case "paste":
    let pid = pid_t(args[2])!
    let pb = NSPasteboard.general; pb.clearContents(); pb.setString(args[3], forType: .string)
    ensureFront(pid)
    let cd = CGEvent(keyboardEventSource: nil, virtualKey: 55, keyDown: true); cd?.type = .flagsChanged; cd?.flags = .maskCommand; post(cd); usleep(15_000)
    let down = CGEvent(keyboardEventSource: nil, virtualKey: 9, keyDown: true); down?.flags = .maskCommand; post(down)
    usleep(40_000)
    let up = CGEvent(keyboardEventSource: nil, virtualKey: 9, keyDown: false); up?.flags = .maskCommand; post(up)
    usleep(15_000); let cu = CGEvent(keyboardEventSource: nil, virtualKey: 55, keyDown: false); cu?.type = .flagsChanged; cu?.flags = []; post(cu)
case "cmdhover":
    // cmdhover <pid> <x> <y> <file.png> [click]: hold cmd (flagsChanged), move
    // onto the point, capture, optionally cmd-click there, release cmd.
    let pid = pid_t(args[2])!; let p = point(pid, Double(args[3])!, Double(args[4])!)
    let clickToo = args.dropFirst(6).contains("click")
    let waitMs = (args.count > 6 ? UInt32(args[6]) : nil) ?? 400
    ensureFront(pid)
    let w = window(of: pid)
    let cd = CGEvent(keyboardEventSource: nil, virtualKey: 55, keyDown: true); cd?.type = .flagsChanged; cd?.flags = .maskCommand; post(cd); usleep(60_000)
    let off = CGPoint(x: p.x + 3, y: p.y - 40)
    let m0 = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: off, mouseButton: .left); m0?.flags = .maskCommand; post(m0); usleep(120_000)
    let m1 = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left); m1?.flags = .maskCommand; post(m1); usleep(waitMs * 1000)
    let cap = Process(); cap.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    cap.arguments = ["-x", "-o", "-l", String(w.id), args[5]]
    try! cap.run(); cap.waitUntilExit()
    if clickToo {
        let d = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left); d?.flags = .maskCommand; post(d)
        usleep(150_000)
        let u = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left); u?.flags = .maskCommand; post(u)
        usleep(100_000)
    }
    let cu = CGEvent(keyboardEventSource: nil, virtualKey: 55, keyDown: false); cu?.type = .flagsChanged; cu?.flags = []; post(cu)
    print(args[5])
case "shot":
    let pid = pid_t(args[2])!; let w = window(of: pid)
    let p = Process(); p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    p.arguments = ["-x", "-o", "-l", String(w.id), args[3]]
    try! p.run(); p.waitUntilExit()
    print("\(args[3]) \(Int(w.w))x\(Int(w.h))")
default:
    die("unknown command \(args[1])")
}
