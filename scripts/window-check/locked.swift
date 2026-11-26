import CoreGraphics
let d = CGSessionCopyCurrentDictionary() as? [String: Any] ?? [:]
print("locked:", d["CGSSessionScreenIsLocked"] ?? "nil", "onConsole:", d["kCGSSessionOnConsoleKey"] ?? "nil")
