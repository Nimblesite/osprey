# Talon Bank for Android

The banking web app's Osprey `App`, model, updates, callable effect handler,
validation, view and HTTP commands run as native code. Kotlin renders the shared
semantic view with Android buttons, editable fields, spinners and scrolling
cards. There is no WebView or second banking implementation.

Requires JDK 17+, Android SDK 34, NDK 28.2 and a phone/emulator on Android 8+.
`scripts/android-env.sh` discovers `ANDROID_HOME` / `ANDROID_NDK_HOME` or their
usual installation locations. The checked-in Gradle launcher verifies its
download checksum.

From the repository root:

```sh
bash examples/projects/modules/mobile/android/build.sh
# Start the bank API on 18790, then install and launch:
bash examples/projects/modules/mobile/android/run.sh
```

Both arm64 phones and x86_64 emulators are packaged by default. Set
`OSPREY_ANDROID_SLICES=x64` for an emulator-only development build.
`OSPREY_ANDROID_SERIAL` selects a device when several are attached.

The default API is `http://127.0.0.1:18790`; the launch script installs
`adb reverse tcp:18790 tcp:18790`. The server connection action in the navigation
drawer accepts a configurable HTTPS API origin. Long-pressing the top bar opens
the same connection dialog. Local emulator aliases also support HTTP.

Run the real device tests against an isolated live ledger:

```sh
python3 scripts/bank-mobile-test.py -- \
  bash examples/projects/modules/mobile/android/test.sh
```

Tests drive actual Android widgets through JNI and the shared Osprey effects:
account creation with Unicode, deposits, withdrawals, atomic transfers,
overdraft refusal, balance/journal checks, activity filtering and search,
validation, modal focus/cancellation, repeated submission, Android Back,
offline recovery and orientation recreation with input drafts preserved.
Reports and screenshots are written beneath `build/`.

The host retains outstanding requests across rotation. Process restoration
refreshes the authoritative ledger and never repeats a pending mutation.
Requests have connection/read timeouts, bounded responses and no automatic
redirects or write retries. Native calls are serialized on the UI thread; JNI
passes standard UTF-8 bytes so supplementary Unicode survives round trips.
