import Foundation
import CoreGraphics

// List only windows belonging to our isolated test process.
let pid = Int(CommandLine.arguments[1])!
let list = CGWindowListCopyWindowInfo([.optionAll, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
let selected = list.filter { ($0[kCGWindowOwnerPID as String] as? Int) == pid }
for window in selected {
    guard let id = window[kCGWindowNumber as String] as? Int else {continue}
    let bounds = window[kCGWindowBounds as String] as? [String: Any] ?? [:]
    print(id, window[kCGWindowLayer as String] ?? "", bounds["Width"] ?? 0, bounds["Height"] ?? 0, window[kCGWindowIsOnscreen as String] ?? false)
}
