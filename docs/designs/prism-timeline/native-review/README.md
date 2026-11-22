# Prism on macOS [PRISM-TIMELINE-NATIVE-REVIEW]

Silt Relay has 19 tracks. These captures follow a short percussion roll, a modular patch and a track that reaches Master through three paths. They come from the running FerroUI app at 2× display resolution.

## 1. Select a note on the Timeline

Shale ticks plays 17 notes across bars 3–4. Select the last hit of the rising roll and the inspector shows C7, MIDI 96, at beat 15.92 with 65% velocity. Track names and instruments now occupy separate lines. Changing another track’s gain on Bus Routing brings its lane into view on return and clears the previous track’s selected note.

![The Shale ticks piano roll with its final C7 selected](piano-roll.webp)

## 2. Open a track patch

Sub current splits its oscillator signal between a dry connection and a fractional delay before the mixer. Its saved card positions leave the sockets, toolbar and module drawer accessible. The native event test also drags a module and exercises project Undo and Redo across pages.

![Sub current patch with the oscillator, delay, mixer, gain and output visible](patch.webp)

## 3. Play the docked keyboard and inspect a module

Launching the studio without a song opens the instrument canvas. From either song page, View → Modules, Patch Controls or Keyboard opens the selected track’s patch with that tool visible. Choosing it again on the patch toggles it. Playing a key changes the note name, frequency and highlighted key. Patch Controls scrolls independently so a musician can reach the last connection and plug-in controls with the keyboard docked.

![The standalone instrument with its filter selected and keyboard docked](studio-keyboard.webp)

[Selected module and parameters](controls.webp) · [Connections and complete Add component picker after scrolling](controls-scrolled.webp)

## 4. Trace a track to the speakers

Rivet pulse feeds Objects, sends 19% to Space, and taps an internal stereo outlet at 6% into Undertow. Selecting the Space and Objects cards changes the inspector while keeping the dry path visible.

![Rivet pulse with separate connections to Undertow, Objects and Space above Master](routing.webp)

## 5. Edit a send or group output

The send list identifies the source outlet and destination. The editor shows the same outlet’s internal connection beside its external send. New sends expose the source, destination, level and mute control. Group output assignment shows the resulting path to Master. The native tests create a named group, save routing into the project, cancel an edit, and undo and redo a send.

[Send list](send-list.webp) · [Edit existing send](edit-send.webp) · [Add send](new-send.webp) · [Group outputs](group-output.webp) · [Create a stereo bus](new-group.webp)

## 6. Open a project, play, stop and recover from a rejected render

File → Open Song opened a separate kick audition. Transport → Play Song rendered the opening two bars of the full Silt Relay project as 320,056 stereo frames at 48 kHz, including the effects tail. Stop cancelled an in-progress render. A deliberately invalid patch produced a readable footer naming the track and failed module. The transport regression also passed with an inline error, a multiline host transcript and an empty transcript. These checks used disposable project copies.

[Renderer diagnostic from the native window](render-diagnostic.webp)

## 7. Scan a VST3 and open its controls

The SDK’s ADelay plug-in scanned, opened as a patch module and exposed Bypass, Delay and audio connections. TAL-NoiseMaker scanned on the same machine. Adding it stopped at initial processor/controller state synchronization, before a module or audio connection was created. The Prism lead has the reproduction and capture.

[ADelay module and native controls](plugin-controls.webp)

## Input timings

The native event tests measured 119 timeline taps, 136 mixer fader events and 55 patch drag events on this Mac. On the build opened by the macOS DAW Make target, mean app processing time was 1.42 ms, 1.97 ms and 3.23 ms respectively. The slowest event in the three groups took 12.37 ms. Both native test suites passed. Display presentation and audio output latency are separate measurements.

## Verification limits

The desktop session exercised pointer input, the menu bar, shortcut presets, keyboard audition, and scrolling at 1152 × 752 window points. Screen-reader announcement order, Windows rendering and third-party plug-in audio were outside this run. Four cases failed again in the separate host-free suite covering cable hit areas, module inspection, plug-in discovery and device status reporting. The Prism lead received the failing case names and output on TMC.
