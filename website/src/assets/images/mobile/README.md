# Issue Inbox screenshots

These are unmodified captures of the running native apps on 7 September 2026. They show public `swiftlang/swift` issues fetched from GitHub by the shared Osprey application. The Android inbox shows data restored from SQLite after restarting the app. Both detail captures follow a live refresh and show Markdown rendered by Osprey.

| File | Capture |
| --- | --- |
| `issue-inbox-ios.png` | iPhone 17 Pro simulator, 1206 × 2622; inbox after a live GitHub refresh |
| `issue-inbox-android.png` | Pixel 7 Android emulator, 1080 × 2400; cached inbox |
| `issue-inbox-android-detail.png` | Pixel 7 Android emulator, 1080 × 2400; issue details, rendered Markdown, and local triage controls |
| `issue-inbox-ios-detail.png` | iOS simulator, 1206 × 2622; the same detail screen in native light appearance |

The iOS application was also signed, installed, launched, and verified on a physical iPhone 16. These website images are simulator and emulator captures, not photographs of that phone.

To refresh the assets, run the apps using [the mobile guide](../../../../../examples/mobile/README.md), wait for a successful refresh, and capture their actual windows. Use `xcrun simctl io <simulator-id> screenshot <file.png>` on iOS and `adb -s <device-id> exec-out screencap -p > <file.png>` on Android. Keep the screenshot files shared by the website and repository READMEs.
