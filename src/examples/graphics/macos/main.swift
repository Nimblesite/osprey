import Foundation

// Implements [IOS-TARGET-ENTRY] and [IOS-TARGET-MACOS]. The scene is the
// program: Osprey's `main` opens the window, animates it until it closes,
// reports its frame rate and returns 0 for a clean run.
exit(osprey_main())
