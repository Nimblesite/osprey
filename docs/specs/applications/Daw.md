# The daw [DAW]

Status: in progress. The project file, its encoder and the transport are implemented and tested. The Timeline, Mix and patch pages are being built on the FerroUI host. The [delivery plan](../../plans/daw.md) records what is verified.

The daw is the [Prism](../../../src/osprey/examples/daw/PRISM.md) patch studio with a song around it. A song project holds tracks of notes. Every track plays through a patch and feeds a stereo Master and a shared reverb, Space. The app opens a project, shows its tracks on a timeline and lets you edit a track's notes, mix and patch. It plays the song as it currently stands and saves everything back to the same file.

## One project file [DAW-PROJECT]

A song is one JSON file, and nothing about a song lives anywhere else. It contains the tempo, sample rate, master drive, Space settings, an optional intro recording, sustains, and every track. Each track stores its notes, gain, pan, Space send and the complete patch it plays.

- **One model ([DAW-PROJECT-MODEL]).** [daw.td](../../../src/osprey/examples/daw/daw.td) declares the project. The Osprey types are generated from it. The JSON keys are the typeDiagram field names. The editor and the file both follow this model; neither keeps a parallel format.
- **Patches in the project ([DAW-PROJECT-PATCH]).** A built-in instrument opens in Prism as one voice node wired to the output. A routed patch opens as its own graph. A patch edited in Prism is written back into its track in the project file. Opening the file again restores the same graph, its pitch and velocity bindings, and its cables.
- **Lossless round trip ([DAW-PROJECT-ROUNDTRIP]).** `SongFiles::encode` writes every key that `SongFiles::decode` reads. A decoded project re-encodes to identical text. A JSON encoding failure is an error, never a `"null"` file.
- **Checked edits ([DAW-PROJECT-EDIT]).** Gain must be 0–4, pan −1 (left) to 1 (right), and the Space send 0–1. A refused edit, or an edit to a track that does not exist, leaves the project unchanged and says why.
- **Saving ([DAW-PROJECT-SAVE]).** Save reads its own encoded text back before writing. A project that would not reopen is never written over the old file. A path containing a quote is refused. Osprey has no file-rename primitive, so the final write is not atomic.

## Transport [DAW-TRANSPORT]

**Play** renders and plays two bars, starting at the first bar shown in the piano roll. It uses the project as it currently stands, including unsaved edits.

1. The app encodes the current project and stores it as `project.json` in a per-bar folder beside the renderer (`<song>/bar-N/`).
2. The renderer `daw <project.json> <folder> <from-beat> <beats>` renders only that window to `piece.wav`. It plays notes starting inside the window, moved to its start, at full length. The intro and sustains are left out.
3. Audio counts only when the renderer actually wrote a WAV. The renderer reports a refused project on stdout and still exits cleanly, so the app shows its message instead of playing.
4. Each ticket carries the folder it was launched for. If the piano roll moves or another song opens during a render, the bars that were launched are still the ones that play.
5. If the stored snapshot equals the current project, the cached bars replay without rendering. Any edit renders again. Before a render, the old audio is emptied so it can never pass for the new snapshot.
6. **Stop** cancels the render or playback in flight. While anything is in flight, Play refuses with "The song is already playing; Stop first."

Rendering is offline and slower than realtime. With the renderer built under ARC, four beats of the codex song take about 32 s. The per-note patch voices dominate that time.

## Pages [DAW-PAGES]

One window shows three views of one session. **Timeline** shows the tracks, the song overview and a two-bar piano roll for the selected track. **Mix** shows each track's gain, pan, Space send and its output, which is Master. **Patch** is Prism showing the selected track's patch. Tapping a track selects it. **Open patch** shows that track's patch. Leaving the patch page first saves the patch into the project; if the save fails, the patch stays open with the reason.

## Scope [DAW-SCOPE]

The [Timeline designs](../../designs/prism-timeline/README.md) propose more than this application ships. Only the parts with a clear use are in scope:

- **Included:** the timeline and piano roll; per-track gain, pan and Space send into Master; one patch per track; play, stop and save.
- **Not built:** nested racks, groups, arbitrary-port sends, creating buses, and a detached second window. Each adds a routing model the renderer cannot play yet. FerroUI multiwindow is deferred.
- **Renderer limits:** the renderer is mono per track before panning and allows at most 64 tracks. Its limits are documented in the [daw README](../../../src/osprey/examples/daw/README.md).
