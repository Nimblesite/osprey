// Prints the window number, width and height of every normal window owned by the named process.
import CoreGraphics
let name = CommandLine.arguments[1]
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
for w in list where (w["kCGWindowOwnerName"] as? String ?? "") == name && (w["kCGWindowLayer"] as? Int ?? 1) == 0 {
  let b = w["kCGWindowBounds"] as! [String: Any]
  print(w["kCGWindowNumber"]!, b["Width"]!, b["Height"]!)
}
