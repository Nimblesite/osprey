# Real application captures

| Website asset | Source |
| --- | --- |
| windows-storage.png | examples/storage/goldens/windows/100/1-listing.png |
| windows-flock.png | examples/gpui/flock/goldens/windows/100/1-flying.png |
| windows-painting.png | examples/gpui/painting/goldens/windows/100/1-painting.png |
| windows-casting.png | examples/casting/goldens/windows/100/1-searched.png |
| macos-painting.png | examples/gpui/painting/Screenshot.png |
| macos-agent.png | examples/agent/goldens/macos-26/01-ready.png |
| macos-synth.png | Fresh window-only capture of Prism Synth.app, 3 November 2026 |
| web-bank.png | Fresh Chromium capture, 1440 × 960, local Talon Bank, 3 November 2026; waits for data-osprey-ready=true |

PNGs preserve their source captures. Windows images come from WinUI self-tests with fixture data. The page reuses mobile simulator/emulator captures documented in ../mobile/README.md, the macOS Casting tutorial capture and the VS Code package guide capture. Captions identify the actual platform; these images do not establish support for other applications or hosts.

The web-bank-accounts, web-bank-move, web-bank-security, web-bank-activity and web-bank-filter captures come from the same local Chromium session as web-bank. The filter capture types vintage into Activity search. No mutation form was submitted.

`android-casting.png` is the native emulator capture from `examples/casting/build/android-test/imported.png`, after the verified MP3 import journey. `ios-casting.png` is the non-failure `casting-ios-ready` XCTest attachment from `Test-Casting-2026.11.03_19-30-33-+1100.xcresult`. The UI journeys do not establish full mobile playback integration; the showcase links the delivery checklist beside these captures.
