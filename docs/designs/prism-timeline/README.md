# Prism Timeline and routing [PRISM-TIMELINE]

The Timeline is a second window of the combined Prism DAW. Select a track to inspect its notes, open its patch, or trace it all the way to the speakers. The three views share the same track and playhead. The current FerroUI host can present the same workspace in its main window until it supports detaching it.

## Running FerroUI window

Follow Silt Relay through the piano roll, patch studio, keyboard and bus mixer in the [native screen review](native-review/README.md).

The native Timeline shows keysL in bars 9–10 of *Broken Circuitry: Reassembly*, with G♯5 selected at beat 33 and velocity 75%. The Mix capture follows glow’s dry path through the pads bus, keyed by kick, and its separate 60% Space send. The patch capture shows a saved Delay 6 between the waveshaper and Output 5 in a previously edited copy of the same song. [Piano roll](../../../src/osprey/examples/daw/screens/timeline.png) · [Bus routing](../../../src/osprey/examples/daw/screens/mix.png) · [Reopened track patch](../../../src/osprey/examples/daw/screens/track-patch.png)

In the running Timeline, the destination chooser shows Master checked and Keys bus focused while G♯5 stays selected. After routing keysL and keysR to the new group, the Mix inspector lists both tracks as inputs. Each retains its 35% Space send. Selecting the Space card reveals the shared reverb’s 6.50-second decay. [Output chooser](../../../src/osprey/examples/daw/screens/assign-output.png) · [Keys bus inputs and signal flow](../../../src/osprey/examples/daw/screens/group-routing.png) · [Space return](../../../src/osprey/examples/daw/screens/space-return.png)

The native send inspector opens over the canvas at its current zoom. On keysL, the loss filter still feeds the waveshaper while its audio outlet also sends 25% to pads. The inspector shows both destinations. [Add a send in Track patch](../../../src/osprey/examples/daw/screens/add-send.png) · [Edit the parallel send in Bus routing](../../../src/osprey/examples/daw/screens/edit-send.png)

Dragging the keysL velocity stem down changed G♯5 to 25%. Moving the note half a beat right and two semitones up kept its 0.30-beat length. The reopened project shows A♯5 at beat 33.50. [Edited piano roll in the native window](../../../src/osprey/examples/daw/screens/dragged-note.png)

In Bus routing, keysL feeds Keys bus, which feeds pads. Selecting the group’s output opens its full path through both groups to Master. [Group outputs in the running app](../../../src/osprey/examples/daw/screens/bus-outputs.png)

On keysL, the Patch sends inspector shows four sends from the loss filter. Three are active and the 75% send to Keys bus is muted. Each entry opens its own saved route. The paging controls reach the fourth send while the track graph stays in view. [Patch sends in the native window](../../../src/osprey/examples/daw/screens/patch-sends.png)

**Edit → Undo** and **Redo** follow the open project across all three pages. For example, move a note, open another track’s patch, then undo. The saved note returns to its previous pitch and time. A second Undo can restore its independently edited velocity. A refused save keeps the same edit available to retry.

## Patch controls [DAW-CONTROLS-DIAL]

Use the Instrument dial across Prism. Its recessed violet face sits inside a 270° track with a warm orange fill and a pale radial indicator. Compact module cards, Patch Controls and the Instrument face keep the same proportions at their control sizes. The focus ring sits outside the track, so the fill and indicator stay visible when the dial takes focus. [Instrument reference](instrument-play.png)

The panel and module card share the same Drive value on keysL’s Waveshaper 4. The example opens at 1.40×, with zero bias and an output level of 0.60. Curve selects a named shape. Antialiasing and oversampling are switches. The ports show the actual connection from Loss filter 3 through Waveshaper 4 to Output 5. [Design screenshot](patch-controls.png) · [Fine adjustment in the inspector](dial-detail.png) · [Interactive dial prototype](patch-controls.html)

Press the dial or its number and drag vertically. A 180-pixel upward drag traverses the range, and Shift reduces sensitivity tenfold. Sideways movement leaves the value in place. The orange arc starts at the minimum. Drive and decay use a logarithmic scale to give small values usable travel. On the keyboard, arrows step the value and Enter opens the numeric field. Double-press restores the module default, Escape cancels a drag, and one completed drag is one undo step. The prototype keeps edits in memory. The native implementation shares each parameter value with the project.

## Instrument builder [PRISM-INSTRUMENT]

The three proposed Instrument views use the Hook track’s FM pluck in *Lagos After Rain*. Play shows the same four controls at their saved sizes. Edit adds the spacing grid, resize handles and the selected control’s targets. Brightness opens the filter while decreasing high damping in the short room. MIDI learn waits for a controller movement. The OSC address stays visible and copies with one action. [Instrument builder specification](../../specs/synth/Perception.md#instrument-builder-prism-instrument)

| Design screenshot | What to inspect |
| --- | --- |
| [Play](instrument-play.png) | Three knobs and a Room switch on the instrument face. |
| [Edit and macro mapping](instrument-edit.png) | The selected knob’s two targets, opposite ranges and independent curves. |
| [MIDI learn and OSC address](instrument-learn.png) | The listening state, cancel action and copyable OSC address. |

## Window commands [PRISM-TIMELINE-MENU]

The agreed menu layout applies to Patch, Timeline and Mix. File holds Open Song, Save and Render Song. Edit holds Undo and Redo. View switches pages and toggles Modules, Patch Controls and Keyboard. Transport holds song playback and patch audition. The three page tabs and Play and Stop stay within reach on the canvas. The current page, project title and playback state remain visible when the menus close.

Click **Output** in the Timeline inspector or the output name in a Mix row to choose a destination in place. The chooser marks the current route and pages through every bus in the open project. Arrow keys move focus, Enter assigns the focused output, and Escape dismisses the chooser. **New stereo bus…** creates a named stereo group and assigns the selected track in one project save. The Routing menu offers the same destinations. In the bundled project, both glow and tablepad feed pads, kick keys its compressor, and the tracks’ independent Space sends return directly to Master.

Verify the change through the native menu and its keyboard shortcuts, including after opening another project. Run the video journey through the same menus, check that clicks on former toolbar positions have no effect, and capture the menu and the resulting Mix view. The Prism lead owns the native menu exporter and Patch page. Timeline and Mix use the same command layout.

## Screens

| Design screenshot | What to inspect |
| --- | --- |
| [Track patch](track-patch.png) | The keysL voice splits into a clean path and a modulated filter rack. A mixer rejoins them. Track gain feeds both a stereo group and a send. |
| [Add a send from an internal port](add-send.png) | Filter rack already feeds Mixer B. Start a second cable from that outlet, choose Echo, and set its level. **Connect send** publishes the extra branch. Cancel discards the preview. |
| [Bus routing](bus-routing.png) | All 17 tracks with their output, pan, gain and Space send. Selecting keysL traces its dry signal and reverb return into Master. Selecting Space reveals the shared effect settings. |
| [Piano roll](piano-roll.png) | The song overview holds the two-bar viewport. The selected G♯5 starts at beat 33 on keysL. The note and its velocity stem share a selection. |
| [Choose a destination](assign-output.png) | Change keysL’s main output while viewing its notes. Master is checked. Keys bus has focus. New stereo bus creates a group and assigns it in one edit. |
| [Group routing](group-routing.png) | After assigning both keys tracks, the group lists both inputs. Their independent Space sends still reach the reverb return. |

The captures use the real **Broken Circuitry: Reassembly** arrangement at 97.5 BPM, Prism's skin assets and palette, and 1-based bar and beat labels. The 4/4 ruler is a display convention for this song. The named Keys group, parallel filter branch and Echo destination show proposed edits. [Song data](../../../src/osprey/examples/daw/tracks/codex-version.json) · [Prism skin](../../../src/osprey/packages/signal_flow/skins/prism)

## One track, one patch [PRISM-TIMELINE-GRAPH]

The design model has **no fixed slot count** for processors, internal cables, sends, buses or nesting depth. A track owns a graph instance. A patch can be one instrument or a network of nested racks, parallel chains, sidechains, event paths and a single main audio Output. Internal outlets can also feed named sends. A bus owns a graph instance too, so a group or return can process audio and feed other buses. Grouping moves selected modules behind named boundary pins while preserving their cables. Opening a rack restores the view of its internals. [Routing model](routing.td)

| Concept | Contract |
| --- | --- |
| Project and track | The session root instantiates tracks, shared buses and the output device. The project declares its speaker layout once. Track, bus, Space and Master audio sockets inherit the same ordered speakers. A lane addresses its track instance and note inlet by stable ID. Renaming or reordering a track, bus or pin preserves routes. |
| Voice and track processing | A Voices node creates a voice instance per struck note. Pitch and velocity drive the declared parameter bindings. Note-off releases the envelope. The voice Output sums like channels across voices before entering the persistent track graph. Track effects and return tails continue between notes. A built-in DAW instrument can occupy a voice without expanding its internal DSP. |
| Instances and racks | A definition declares typed public pins. Each public pin maps to a boundary node of the opposite internal direction. An instance exposes that interface to its parent. Two rack instances own independent processor state. A shared return has one instance that receives all its sends. |
| Patch cables | Every wire joins a fixed, typed outlet to a compatible inlet. Drag again from an occupied outlet to fan out the same signal at unity gain. Use the distinct inlets of a Sum or Mixer to combine sources. A full mixer can feed another mixer. Inside a voice, sockets retain the synth port’s rate, channels and unit. |
| Between tracks | The destination chooser lists compatible project audio, MIDI and OSC inputs on tracks and buses. The chosen row names its owner, socket and signal type. A MIDI output can drive another track’s note inlet while the first track’s audio continues to its bus. Master is the project’s final audio output. |
| Outputs and sends | Main output reassigns a track's designated dry boundary port. A send splits any compatible internal output through an independently enabled gain to a bus inlet. Pre-fader, post-fader and pre-pan describe the chosen socket’s position in that graph. A return has its own output destination. |
| Cross-boundary routing | Creating a send inside a rack exports its tap through stable pins up to the lowest common parent. A wire there connects to the destination's input path, through a sum inlet when needed. The UI traces that path and can collapse the boundary hops. A sidechain follows the same rule with a processor's declared sidechain inlet. |
| Feedback | Definition references form an acyclic containment hierarchy. Expand instance boundaries and validate the combined signal graph, including bus-to-bus returns. A feedback cycle needs a positive explicit Delay accepted by the synth scheduler. A delayed effect's name is insufficient. Recursive rack references are rejected. |
| Edit and recovery | Rewire, create-and-assign, duplicate and group are single revision-checked transactions. Validate the candidate before publication. Show the offending socket, incompatible channels, occupied inlet, cycle path or capacity failure. The previous patch stays audible. Deleting a bus lists affected connections and requires an explicit replacement or disconnect. Undo restores identities, connections and levels. |

The typeDiagram describes how these connections belong in the single project JSON. The current native chooser connects audio to buses. Track-to-track MIDI and OSC destinations are specified for implementation. [Project graph](../../specs/synth/Perception.md#the-project-is-a-graph-daw-graph)

The graph document composes the existing synth node and port types with the DAW instrument and note types. The editor can address nested objects using their instance path and local node/pin IDs. Pan, zoom, selection and node positions belong to the editor. They do not affect routing or scheduling. [Synth graph](../../../src/osprey/packages/synth/graph.td) · [DAW arrangement](../../../src/osprey/examples/daw/daw.td)

## Reading and editing a large patch [PRISM-TIMELINE-NAVIGATION]

- **Timeline.** Click the note area of a lane to inspect its piano roll. Tap the track header or choose **Track patch** to open that track’s Prism patch. Selecting a different track releases the old note selection. Reopening the window restores its last viewport.
- **Track patch.** Drag from a socket or select its outlet and choose a compatible inlet in the inspector. Click a cable to reveal both endpoints. **Find** searches node names, port names and bus names across nested instances. Selecting a match reveals the rack path and fits that node. The overview and **Fit** command work across the full graph.
- **Bus routing.** The table projects the project graph. The send-column picker selects a bus. **+ Send** chooses a source tap, destination and amount. If multiple taps on the same track reach one bus, the cell shows the send count and expands into one row per tap with its own level and source path. **Buses** opens the bus list, including groups and returns, their inputs, and their onward routes.
- **Space to work.** Use a resizable split between arrangement and piano roll, independent track and pitch scrolling, and a collapsible inspector. The piano roll follows the highlighted time range. At narrow widths the routing inspector opens as a drawer. Keyboard selection, visible focus, socket names and line styles carry the same information as color.

## Integration in the combined app [PRISM-TIMELINE-INTEGRATION]

1. The unified DAW owns one arrangement and graph revision. **Timeline** in the studio opens or focuses the companion window. **Show studio** returns to the selected track's patch. For the single-window host, these are view switches with the same selection and history.
2. Materialize the current song faithfully. Map each routed instrument’s nodes, cables, note bindings, attack and release into its voice graph. Its output, in the project's channel layout, feeds the track gain. One branch pans to the stereo mix and another branches before pan into Space at the saved send level. Preserve zero sends and reverb settings. Nothing is added to Master. Import built-in voices with their original parameters. The session master sums stereo dry and wet, then limits the mix. [Mix renderer](../../../src/osprey/examples/daw/src/arrangements.ospml)
3. The synth and FerroUI owners agreed on tap-to-patch, a common FerroUI surface across desktops, and the current Master/Space mapping as the first integration milestone. Named group buses, their EQ and sidechain compression, and assigning a track to a group now work in Mix. Individual internal mono or stereo outlets now send to named groups with independent levels, including multiple taps on the same track. Named groups can feed other groups and the Bus outputs inspector shows the route to Master. Nested patch racks remain design work.
4. The Timeline, patch canvas and bus table dispatch edits to that same graph. Dragging a wire or editing a table cell updates both views. The host presents the shared Prism surface and routes input. The graph engine owns validation, preparation and activation.
5. Project patches accept variable-length node and cable lists. The scheduler validates occupied inputs, typed ports and feedback, then orders every reachable dependency and disconnected component. A 2,052-node fan-out passes the package tests, and a saved 258-node chain of unity gains renders the same stereo samples as its direct connection. The offline clip engine sizes its memory budget by block length and channel count. MIDI, control and multichannel audio run at 48 kHz through that engine. A stereo track sink is averaged before the track panner. [Graph scheduler](../../../src/osprey/packages/synth/src/graph.ospml) · [Clip storage](../../../src/osprey/packages/synth/src/clip_layouts.ospml) · [DAW renderer](../../../src/osprey/examples/daw/src/patch_voices.ospml)

**Integration walkthrough.** Open keysL at bar 9, inspect G♯5, then enter its patch. Split the voice, add a modulated parallel branch, mix it back, and tap its output into Echo. Assign keysL and keysR to Keys bus. Trace each dry route, both Space sends, and the new Echo send to Master. Switch back to the roll and verify the note and playhead. Undo the Echo send and confirm the internal mix and existing sends survive. Exercise nested buses, a refused zero-delay loop, and reopening the detached window against the same session revision.
