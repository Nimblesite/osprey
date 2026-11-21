//! The daw's Timeline tap budget, end to end through the real shared-memory boundary
//! ([NATIVE-SHM-EVENT], [DAW-PAGES]): the linked daw opens a real song project on its Timeline, lanes
//! are tapped across every track, the ruler is tapped, every Mix gain fader is dragged, and each event
//! must leave the application's `event` export within one 60 Hz display frame. A separate file from `prism_events.rs` because the linked application
//! holds one session per process and this one opens with a song.
//!
//! Needs the daw linked: `scripts/prism.sh` builds `target/prism-app/libOspreyApp.a`, and the test
//! runs with `OSPREY_NATIVE_APP_DIR` naming it (README.md).

use std::sync::atomic::Ordering;

use ferroui_osprey_native::events::Touch;
use ferroui_osprey_native::ffi::{self, Library};
use ferroui_osprey_native::regions::Regions;
use ferroui_osprey_native::rows::Shown;
use ferroui_base::input::KeyGesture;
use ferroui_osprey_native::tree::Tree;
use ferroui_osprey_native::host::Menu;
use ferroui_osprey_native::view::window_menus;

/// One display frame at 60 Hz, in milliseconds.
const FRAME_MS: f64 = 1000.0 / 60.0;
/// The song's tracks, and the first taps left out of the budget while caches fill.
const TRACKS: usize = 17;
const WARM_UP: usize = 3;
/// The shapes the Timeline draws for the song, at least.
const TIMELINE_SHAPES: usize = 300;
/// The shapes the Mix page draws for the song, at least, and the middle of its header tab.
const MIX_SHAPES: usize = 200;
const MIX_TAB: (f64, f64) = (560.0, 29.0);

fn repo(path: &str) -> String {
    format!("{}/../../../{path}", env!("CARGO_MANIFEST_DIR"))
}

/// The control number of the first scene at or below `node`, depth first.
fn scene(node: &Tree) -> Option<i64> {
    match node {
        Tree::Record(members) => match members.get("kind") {
            Some(Tree::Text(kind)) if kind == "scene" => match members.get("event") {
                Some(Tree::Whole(ordinal)) => Some(*ordinal),
                _ => None,
            },
            _ => members.get("body").and_then(scene).or_else(|| members.values().find_map(scene)),
        },
        Tree::Items(items) => items.iter().find_map(scene),
        _ => None,
    }
}

fn scene_event(node: &Tree, row: i64) -> Option<i64> {
    match node {
        Tree::Record(fields) if fields.get("sceneRow") == Some(&Tree::Whole(row)) =>
            match fields.get("event") { Some(Tree::Whole(event)) => Some(*event), _ => None },
        Tree::Record(fields) => fields.values().find_map(|child| scene_event(child, row)),
        Tree::Items(items) => items.iter().find_map(|child| scene_event(child, row)),
        _ => None,
    }
}

fn shapes(shown: &Shown) -> usize {
    shown.scenes.values().map(|scene| scene.shapes.len()).sum()
}

/// One pointer event; returns its frame and the milliseconds the application took.
fn point(canvas: i64, (x, y): (f64, f64), down: bool) -> (Shown, f64) {
    match Library::event(0, &Touch::pointer(canvas, x, y, down, false)) {
        Ok(shown) => (shown, ffi::APP_MICROS.load(Ordering::Relaxed) as f64 / 1000.0),
        Err(problem) => panic!("the daw refused a pointer event at {x},{y}: {problem}"),
    }
}

/// The scene point of lane `track` at fraction `along` of the song. Seven rows fit on each page of the arrangement.
fn lane(track: usize, along: f64) -> (f64, f64) {
    (198.0 + along * 894.0, 221.0 + ((track % 7) as f64 + 0.5) * 31.0)
}

/// The window menus the root region names.
fn menus_of(regions: &Regions) -> Vec<Menu> {
    regions.get("root").map(|root| window_menus(&root.node)).unwrap_or_default()
}

/// Every command of `menus` has a shortcut FerroUI reads, and only a command and its counterpart
/// (Play and Stop, which are never available together) share one.
fn shortcuts(menus: &[Menu], keymap: &str) {
    let mut owners: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for command in menus.iter().take(4).flat_map(|menu| &menu.commands) {
        assert!(KeyGesture::parse(&command.keys).is_ok(), "{keymap}: {} has the shortcut {}", command.label, command.keys);
        owners.entry(command.keys.as_str()).or_default().push(command.label.as_str());
    }
    for (keys, labels) in owners {
        let pair = labels == ["Play Song", "Stop Song"] || labels == ["Play Patch", "Stop Patch"];
        assert!(labels.len() == 1 || pair, "{keymap}: {keys} is on {labels:?}");
    }
}

fn keys_of(menus: &[Menu], label: &str) -> String {
    let found = menus.iter().flat_map(|menu| &menu.commands).find(|command| command.label == label);
    found.map(|command| command.keys.clone()).unwrap_or_else(|| panic!("no command {label}"))
}

/// [DAW-MENU] [DAW-KEYS] The daw's menu bar: File, Edit, View, Transport and Shortcuts. Each keymap gives
/// every command a shortcut, choosing one changes the keys and checks it alone, and Save chosen from
/// the menu is not refused.
fn menu_bar(regions: &mut Regions) {
    let menus = menus_of(regions);
    let titles: Vec<&str> = menus.iter().map(|menu| menu.title.as_str()).collect();
    assert_eq!(titles, ["File", "Edit", "View", "Transport", "Shortcuts"], "the window's menus");
    let sizes: Vec<usize> = menus.iter().map(|menu| menu.commands.len()).collect();
    assert_eq!(sizes, [4, 2, 9, 11, 4], "the commands of each menu");
    for (keymap, routing, play, zoom) in [("Logic Style", "X", "Space", ["Cmd+Right", "Cmd+Left", "Z"]), ("Ableton Style", "Cmd+Alt+M", "Space", ["OemPlus", "OemMinus", "Cmd+D0"]), ("Prism", "Cmd+D3", "Space", ["Cmd+OemPlus", "Cmd+OemMinus", "Cmd+D0"])] {
        let choice = menus_of(regions)[4].commands.iter().find(|command| command.label == keymap).map(|command| command.event);
        let Some(choice) = choice else {
            panic!("Shortcuts offers {keymap}");
        };
        let changed = Library::event(0, &Touch::control(choice, "")).expect("choosing a keymap");
        assert!(changed.diagnostics().next().is_none(), "choosing {keymap} is not refused");
        regions.adopt(&changed).expect("the menus after choosing a keymap");
        let menus = menus_of(regions);
        shortcuts(&menus, keymap);
        assert_eq!(keys_of(&menus, "Loop"), "L", "{keymap}: Loop");
        assert_eq!(keys_of(&menus, "Bus Routing"), routing, "{keymap}: Bus Routing");
        assert_eq!(keys_of(&menus, "Play Song"), play, "{keymap}: Play Song");
        assert_eq!(keys_of(&menus, "Stop Song"), play, "{keymap}: Stop Song");
        // [DAW-TIMELINE-ZOOM] Zoom In, Zoom Out and Zoom to Fit are View commands with keys in every keymap.
        for (label, keys) in ["Zoom In", "Zoom Out", "Zoom to Fit"].into_iter().zip(zoom) {
            assert_eq!(keys_of(&menus, label), keys, "{keymap}: {label}");
        }
        let checked: Vec<&str> = menus[4].commands.iter().filter(|command| command.checked).map(|command| command.label.as_str()).collect();
        assert_eq!(checked, [keymap], "only the keymap in use is checked");
    }
    let menus = menus_of(regions);
    assert_eq!(keys_of(&menus, "Save"), "Cmd+S", "Save's shortcut");
    let save = menus[0].commands.iter().find(|command| command.label == "Save").map(|command| command.event);
    match Library::event(0, &Touch::control(save.expect("File has Save"), "")) {
        Ok(saved) => assert!(saved.diagnostics().next().is_none(), "Save from the menu is not refused"),
        Err(problem) => panic!("the daw refused Save from the menu: {problem}"),
    }
}

fn choose_menu(regions: &mut Regions, title: &str, label: &str) -> Shown {
    let menus = menus_of(regions);
    let event = menus.iter().find(|menu| menu.title == title)
        .and_then(|menu| menu.commands.iter().find(|item| item.label == label && item.enabled))
        .unwrap_or_else(|| panic!("missing enabled {title} > {label}")).event;
    let shown = Library::event(0, &Touch::control(event, "")).expect("dispatch menu command");
    regions.adopt(&shown).expect("adopt the command's workspace");
    shown
}

fn named_control<'a>(node: &'a Tree, kind: &str, label: &str) -> Option<&'a std::collections::BTreeMap<String, Tree>> {
    match node {
        Tree::Record(fields) => {
            if matches!(fields.get("kind"), Some(Tree::Text(value)) if value == kind)
                && matches!(fields.get("label"), Some(Tree::Text(value)) if value == label)
            {
                return Some(fields);
            }
            fields.values().find_map(|child| named_control(child, kind, label))
        }
        Tree::Items(items) => items.iter().find_map(|child| named_control(child, kind, label)),
        _ => None,
    }
}

fn patch_region(regions: &Regions) -> &Tree {
    let key = regions.detached().expect("a separate track patch window");
    &regions.get(&key).expect("patch content").node
}

fn choose_control(regions: &mut Regions, kind: &str, label: &str, text: &str) {
    let root = &regions.get("root").expect("workspace").node;
    let fields = named_control(root, kind, label).or_else(|| named_control(patch_region(regions), kind, label)).unwrap_or_else(|| panic!("missing {kind} {label}"));
    let Some(Tree::Whole(event)) = fields.get("event") else { panic!("missing event for {label}"); };
    let changed = Library::event(0, &Touch::control(*event, text)).expect("dispatch routing control");
    regions.adopt(&changed).expect("updated workspace");
}

/// [PRISM-TIMELINE-GRAPH] Create a named group from the piano roll and share it with the adjacent track.
fn create_group(regions: &mut Regions, canvas: i64, song: &std::path::Path) {
    let read = || -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(song).expect("read the song")).expect("decode saved project")
    };
    let original = read();
    let open = |regions: &mut Regions, row: usize| {
        // The start marker intercepts presses along its line. Select each track away from the preceding marker.
        let at = lane(row, 0.21 + 0.04 * row as f64);
        point(canvas, at, true);
        point(canvas, at, false);
        point(canvas, (1200.0, 535.0), true);
        regions.adopt(&point(canvas, (1200.0, 535.0), false).0).expect("open the inline chooser");
    };
    open(regions, 5);
    let pick = |regions: &mut Regions, y: f64| {
        point(canvas, (1190.0, y), true);
        regions.adopt(&point(canvas, (1190.0, y), false).0).expect("choose from the inspector");
    };
    pick(regions, 709.0);
    choose_control(regions, "field", "Bus name", "Keys bus");
    assert_eq!(read(), original, "typing does not change the project");
    choose_control(regions, "button", "Cancel", "");
    assert_eq!(read(), original, "cancel leaves the project intact");
    open(regions, 5);
    choose_control(regions, "button", "New stereo bus…", "");
    for invalid in ["pads", " master "] {
        choose_control(regions, "field", "Bus name", invalid);
        choose_control(regions, "button", "Create and assign", "");
        assert_eq!(read(), original, "a refused name leaves the file intact");
    }
    choose_control(regions, "field", "Bus name", " Keys bus ");
    choose_control(regions, "button", "Create and assign", "");
    let mut expected = original.clone();
    for bus in expected["buses"].as_array_mut().expect("existing project buses") {
        bus.as_object_mut().expect("group").entry("output").or_insert(serde_json::json!(""));
    }
    let saved = read();
    let group = saved["buses"].as_array().expect("buses").last().expect("new group");
    assert_eq!(group["name"], "Keys bus");
    assert_eq!(group["gain"], 1.0);
    assert_eq!(group["compressor"]["ratio"], 1.0);
    expected["buses"].as_array_mut().expect("buses").push(group.clone());
    expected["tracks"][5]["bus"] = "Keys bus".into();
    assert_eq!(saved, expected, "only the new group and the chosen dry output changed");
    open(regions, 6);
    pick(regions, 705.0);
    expected["tracks"][6]["bus"] = "Keys bus".into();
    assert_eq!(read(), expected, "both keys tracks share the group and retain their individual sends");
    open(regions, 6);
    for key in ["up", "escape"] {
        Library::event(0, &Touch::key(canvas, key, true)).expect("navigate the open chooser");
    }
    assert_eq!(read(), expected, "Escape discards the focused route");
    open(regions, 6);
    for key in ["up", "enter"] {
        Library::event(0, &Touch::key(canvas, key, true)).expect("commit the keyboard selection");
    }
    expected["tracks"][6]["bus"] = "pads".into();
    assert_eq!(read(), expected, "Enter commits the adjacent destination");
}

fn shows_label(shown: &Shown, label: &str) -> bool {
    shown.scenes.values().flat_map(|scene| &scene.shapes).any(|shape|
        matches!(shape, ferroui_osprey_native::scene::Shape::Lettering { text, .. } if text == label))
}

/// [DAW-MIX-BUS] Choose both destinations from the selected track's native menu.
fn output_routing(regions: &mut Regions, canvas: i64, song: &std::path::Path) {
    let read = || -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(song).expect("read the saved project")).expect("decode the project")
    };
    point(canvas, (665.0, 251.0), true);
    regions.adopt(&point(canvas, (665.0, 251.0), false).0).expect("select glow");
    let mut expected = read();
    assert_eq!(expected["tracks"][0]["bus"], "pads");
    for at in [(900.0, 466.0), (1238.0, 356.0)] {
        let (shown, _) = point(canvas, at, true);
        for label in ["Group bus", "Detector   kick     ·     4.00× compression", "Output     Master   ›", "Stereo out"] {
            assert!(shows_label(&shown, label), "glow's group route shows {label}");
        }
        for label in ["Space", "Shared reverb", "DECAY"] {
            assert!(!shows_label(&shown, label), "the project has no global {label}");
        }
        point(canvas, at, false);
        assert_eq!(read(), expected, "inspecting the group and empty canvas preserves every project field");
    }
    choose_control(regions, "button", "Add send…", "");
    choose_control(regions, "button", "Cancel", "");
    assert_eq!(read(), expected, "cancelling a send keeps the project intact");
    for (label, bus) in [("Master", ""), ("pads", "pads")] {
        let root = regions.get("root").expect("Mix workspace");
        let menus = window_menus(&root.node);
        let routing = menus.iter().find(|menu| menu.title == "Routing").expect("Mix has a Routing menu");
        assert_eq!(routing.commands.len(), 7, "all track destinations, group creation, send list and bus outputs");
        assert!(routing.commands.iter().any(|item| item.label == "Patch sends…" && !item.enabled), "an empty track has no sends to inspect");
        assert_eq!(routing.commands.iter().filter(|command| command.enabled).count(), 5);
        let destination = routing.commands.iter().find(|item| item.enabled && item.label == label).expect("available destination");
        let changed = Library::event(0, &Touch::control(destination.event, "")).expect("choose the output");
        regions.adopt(&changed).expect("redraw the routing diagram and menu");
        expected["tracks"][0]["bus"] = serde_json::Value::String(bus.to_owned());
        assert_eq!(read(), expected, "only glow's dry output changes");
    }
    point(canvas, (665.0, 437.0), true);
    regions.adopt(&point(canvas, (665.0, 437.0), false).0).expect("select keysL");
    choose_control(regions, "button", "Add send…", "");
    choose_control(regions, "picker", "Source outlet", "3/0");
    choose_control(regions, "picker", "Destination bus", "pads");
    choose_control(regions, "field", "Send level (%)", "25");
    choose_control(regions, "button", "Connect send", "");
    expected["tracks"][5]["sends"] = serde_json::json!([
        {"sendId":1,"source":3,"outlet":0,"bus":"pads","level":0.25,"enabled":true}
    ]);
    assert_eq!(read(), expected, "keysL keeps its dry group, notes and patch cables");
    choose_menu(regions, "Edit", "Undo");
    let mut dry = expected.clone();
    dry["tracks"][5]["sends"] = serde_json::json!([]);
    assert!(read() == dry, "Undo removes only the new internal send");
    choose_menu(regions, "Edit", "Redo");
    assert!(read() == expected, "Redo restores its outlet, destination, level and enabled state");
    let (pads, _) = point(canvas, (665.0, 251.0), true);
    assert!(shows_label(&pads, "4 tracks"), "pads receives glow, tablepad, keysR and the new keysL outlet");
    point(canvas, (665.0, 251.0), false);
    point(canvas, (665.0, 437.0), true);
    regions.adopt(&point(canvas, (665.0, 437.0), false).0).expect("return to keysL");
    choose_control(regions, "button", "Add send…", "");
    choose_control(regions, "picker", "Source outlet", "3/0");
    choose_control(regions, "picker", "Destination bus", "Keys bus");
    choose_control(regions, "field", "Send level (%)", "75");
    choose_control(regions, "button", "Mute send", "");
    choose_control(regions, "button", "Connect send", "");
    let second = serde_json::json!({"sendId":2,"source":3,"outlet":0,"bus":"Keys bus","level":0.75,"enabled":false});
    expected["tracks"][5]["sends"].as_array_mut().expect("keysL sends").push(second.clone());
    assert_eq!(read(), expected, "the same outlet feeds two independent groups");
    for id in [3, 4] {
        choose_control(regions, "button", "Add send…", "");
        choose_control(regions, "picker", "Source outlet", "3/0");
        choose_control(regions, "picker", "Destination bus", "pads");
        choose_control(regions, "field", "Send level (%)", "40");
        choose_control(regions, "button", "Connect send", "");
        let tap = serde_json::json!({"sendId":id,"source":3,"outlet":0,"bus":"pads","level":0.4,"enabled":true});
        expected["tracks"][5]["sends"].as_array_mut().expect("keysL sends").push(tap);
    }
    point(canvas, (836.0, 244.0), true);
    regions.adopt(&point(canvas, (836.0, 244.0), false).0).expect("inspect every tap from the graph");
    choose_control(regions, "button", "Next", "");
    choose_control(regions, "button", "Previous", "");
    choose_control(regions, "button", "Edit send 2 · Keys bus…", "");
    choose_control(regions, "field", "Send level (%)", "42");
    choose_control(regions, "button", "Next", "");
    choose_control(regions, "button", "Previous", "");
    assert_eq!(read(), expected, "paging the outlet connections keeps the project unchanged");
    choose_control(regions, "button", "Save send", "");
    expected["tracks"][5]["sends"][1]["level"] = serde_json::json!(0.42);
    assert_eq!(read(), expected, "the pending level survives both navigation directions");
    choose_control(regions, "button", "Patch sends…", "");
    choose_control(regions, "button", "Edit send 2 · Keys bus…", "");
    choose_control(regions, "field", "Send level (%)", "75");
    choose_control(regions, "button", "Save send", "");
    expected["tracks"][5]["sends"][1] = second.clone();
    assert_eq!(read(), expected, "restoring the level changes only this tap");
    choose_control(regions, "button", "Patch sends…", "");
    choose_control(regions, "button", "Edit send 2 · Keys bus…", "");
    choose_control(regions, "button", "Enable send", "");
    choose_control(regions, "button", "Cancel", "");
    assert_eq!(read(), expected, "browsing and cancelling preserves all four independent sends");
    for id in [4, 3] {
        choose_control(regions, "button", "Patch sends…", "");
        if id == 4 {
            choose_control(regions, "button", "Next", "");
        }
        choose_control(regions, "button", &format!("Edit send {id} · pads…"), "");
        choose_control(regions, "button", "Remove send", "");
        expected["tracks"][5]["sends"].as_array_mut().expect("keysL sends").pop();
        assert_eq!(read(), expected, "removing a tap on either page preserves all siblings");
    }
    choose_control(regions, "button", "Patch sends…", "");
    choose_control(regions, "button", "Edit send 1 · pads…", "");
    choose_control(regions, "button", "Remove send", "");
    expected["tracks"][5]["sends"] = serde_json::json!([second]);
    assert_eq!(read(), expected, "removing the first send preserves the muted sibling and its identity");
    choose_control(regions, "button", "Patch sends…", "");
    choose_control(regions, "button", "Edit send 2 · Keys bus…", "");
    choose_control(regions, "button", "Enable send", "");
    choose_control(regions, "field", "Send level (%)", "50");
    choose_control(regions, "button", "Save send", "");
    expected["tracks"][5]["sends"][0]["enabled"] = serde_json::json!(true);
    expected["tracks"][5]["sends"][0]["level"] = serde_json::json!(0.5);
    assert_eq!(read(), expected, "the surviving send responds to its own level and mute control");
    choose_control(regions, "button", "Patch sends…", "");
    choose_control(regions, "button", "Edit send 2 · Keys bus…", "");
    choose_control(regions, "button", "Remove send", "");
    expected["tracks"][5]["sends"] = serde_json::json!([]);
    assert_eq!(read(), expected, "removing both sends restores the complete project");
    choose_control(regions, "button", "Bus outputs…", "");
    choose_control(regions, "picker", "Group bus", "Keys bus");
    choose_control(regions, "picker", "Stereo output", "pads");
    expected["buses"][1]["output"] = "pads".into();
    assert_eq!(read(), expected, "the group goes through pads, preserving track and send destinations");
    choose_control(regions, "picker", "Group bus", "pads");
    choose_control(regions, "picker", "Stereo output", "Keys bus");
    assert_eq!(read(), expected, "a cycle retains the previous route and every audio setting");
    choose_control(regions, "picker", "Group bus", "Keys bus");
    choose_control(regions, "picker", "Stereo output", "Master");
    expected["buses"][1]["output"] = "".into();
    choose_control(regions, "button", "Close", "");
    assert_eq!(read(), expected, "returning the group to Master restores its dry output");
    point(canvas, (820.0, 875.0), true);
    regions.adopt(&point(canvas, (820.0, 875.0), false).0).expect("open group output from the inspector");
    choose_control(regions, "picker", "Stereo output", "pads");
    expected["buses"][1]["output"] = "pads".into();
    assert_eq!(read(), expected, "the group inspector edits the same route as the menu");
    choose_control(regions, "button", "Close", "");
}

// The selected key's scene text needs space for the system typeface below its em height.
fn pitch_label_fits(shown: &Shown) {
    use ferroui_osprey_native::scene::Shape;
    let shapes = shown.scenes.values().flat_map(|scene| &scene.shapes);
    let (left, top, size) = shapes.clone().find_map(|shape| match shape {
        Shape::Lettering { text, x, y, size, .. } if text == "G♯5" && (*x - 209.25).abs() < 1.0
            => Some((*x, *y, *size)),
        _ => None,
    }).expect("the selected G-sharp has a pitch label");
    assert!(shapes.clone().any(|shape| matches!(shape,
        Shape::Rect { x, y, w, h, depth, .. } if *depth == 7.0 && *w > 15.0 &&
            left > x - w / 2.0 && left < x + w / 2.0 && top + size / 2.0 > y - h / 2.0 &&
            top + size * 1.15 < y + h / 2.0
    )), "the complete pitch label fits in its note");
}

/// [DAW-PIANO-DRAG] Drag the stem and note on the canvas, preserving the rest of the project.
fn drag_piano_note(regions: &mut Regions, canvas: i64, song: &std::path::Path) {
    let read = || serde_json::from_slice::<serde_json::Value>(&std::fs::read(song).expect("read project")).expect("project JSON");
    let mut expected = read();
    let original = expected.clone();
    let index = expected["tracks"][5]["notes"].as_array().expect("keysL notes").iter()
        .position(|n| n["start"] == 33.0 && n["pitch"] == 80).expect("G-sharp at beat 33");
    point(canvas, (380.0, 391.0), true);
    point(canvas, (380.0, 391.0), false);
    point(canvas, (206.0, 862.0), true);
    point(canvas, (242.0, 870.0), true);
    let (stem, _) = point(canvas, (255.0, 883.25), true);
    pitch_label_fits(&stem);
    assert!(shows_label(&stem, "25%"), "the inspector previews the stem during the drag");
    assert!(read() == expected, "drag previews never save the project mid-gesture");
    point(canvas, (255.0, 883.25), false);
    expected["tracks"][5]["notes"][index]["velocity"] = 0.25.into();
    assert!(read() == expected, "releasing the stem changes only the grabbed G-sharp to 25%");
    let softer = expected.clone();
    point(canvas, (218.0, 606.0), true);
    point(canvas, (247.0, 597.0), true);
    let (moved, _) = point(canvas, (282.625, 582.363636), true);
    assert!(shows_label(&moved, "MIDI 82") && shows_label(&moved, "33.50"), "the roll previews pitch and time before saving");
    assert!(read() == expected, "the note move is committed only on release");
    point(canvas, (282.625, 582.363636), false);
    expected["tracks"][5]["notes"][index]["start"] = 33.5.into();
    expected["tracks"][5]["notes"][index]["pitch"] = 82.into();
    assert!(read() == expected, "the same note moves half a beat and two semitones, retaining duration and velocity");
    point(canvas, (282.625, 582.363636), true);
    point(canvas, (346.0, 570.0), true);
    Library::event(0, &Touch::key(canvas, "escape", true)).expect("cancel drag");
    point(canvas, (346.0, 570.0), false);
    assert!(read() == expected, "Escape releases the captured note and discards the draft");
    choose_menu(regions, "Transport", "Next Track");
    choose_menu(regions, "View", "Track Patch");
    let backup = song.with_extension("held");
    std::fs::rename(song, &backup).expect("hold the saved project");
    std::fs::create_dir(song).expect("make the project path unwritable as a file");
    choose_menu(regions, "Edit", "Undo");
    std::fs::remove_dir(song).expect("clear the failed save destination");
    std::fs::rename(&backup, song).expect("restore the saved project");
    assert!(read() == expected, "a failed history write preserves the saved project");
    choose_menu(regions, "Edit", "Undo");
    assert!(read() == softer, "Undo from a different patch restores the note's pitch and time on disk");
    choose_menu(regions, "View", "Bus Routing");
    choose_menu(regions, "Edit", "Undo");
    assert!(read() == original, "one more Undo restores only the note's previous velocity");
    choose_menu(regions, "Edit", "Redo");
    assert!(read() == softer, "Redo from Mix restores the released stem");
    choose_menu(regions, "View", "Timeline");
    choose_menu(regions, "Edit", "Redo");
    assert!(read() == expected, "Redo restores the complete saved project after navigating");
    // A mixer gesture on another track must not reinterpret a selected note's index.
    choose_menu(regions, "View", "Bus Routing");
    let snare_gain = (225.0, 235.0 + 14.5 * 37.0);
    point(canvas, snare_gain, true);
    point(canvas, snare_gain, false);
    point(canvas, (330.0, 29.0), true);
    let (returned, _) = point(canvas, (330.0, 29.0), false);
    assert!(shows_label(&returned, "echoes") && shows_label(&returned, "snare"), "scrolling reveals snare in the last row and retains the six lanes above it");
    assert!(!shows_label(&returned, "hat"), "revealing snare scrolls only as far as needed");
    assert!(!shows_label(&returned, "SELECTED NOTE"), "a mixer edit on another track clears the old piano note");

}

// [PRISM-ROUTING-LAYOUT] A card move survives reopening the project before another action can save it.
/// [DAW-INSPECTOR] Read and change a spectral mode by name in the track patch.
fn spectral_picker(regions: &mut Regions, song: &std::path::Path) {
    let copy = song.with_file_name("spectral.json");
    std::fs::copy(repo("osprey/examples/daw/tracks/silt-relay.json"), &copy).expect("copy the spectral project");
    choose_menu(regions, "File", "Open Song…");
    let loaded = Library::event(0, &Touch::completed("daw-open", Ok(copy.display().to_string()))).expect("open the spectral project");
    regions.adopt(&loaded).expect("load timeline");
    let mix = choose_menu(regions, "View", "Bus Routing");
    let read = || serde_json::from_slice::<serde_json::Value>(&std::fs::read(&copy).expect("project")).expect("JSON");
    let project = read();
    let tracks = project["tracks"].as_array().expect("tracks");
    let index = tracks.iter().position(|t| t["name"] == "Throat / metal").expect("spectral track");
    let canvas = regions.get("root").and_then(|r| scene(&r.node)).expect("mix canvas");
    let target = format!("Mix track {}", index + 1);
    let lane = mix.scenes.values().flat_map(|s| &s.access).find(|a| a.label == target).expect("routed track");
    let row = lane.y + lane.h / 2.0;
    point(canvas, (80.0, row), true);
    let patch = point(canvas, (80.0, row), false).0;
    regions.adopt(&patch).expect("open spectral patch window");
    let canvas = scene(patch_region(regions)).expect("spectral patch canvas");
    let card = patch.scenes.values().flat_map(|s| &s.access).find(|a| a.label == "Canvas Spectral processor 2").expect("spectral card");
    let title = (card.x + 55.0, card.y + 28.0);
    point(canvas, title, true);
    let inspector = point(canvas, title, false).0;
    assert!(shows_label(&inspector, "Transform") && shows_label(&inspector, "morph"), "the inspector shows the active transform by name");
    let mode = inspector.scenes.values().flat_map(|s| &s.access)
        .find(|a| a.label == "Spectral processor 2 Transform").expect("transform selector");
    let at = (mode.x + mode.w / 2.0, mode.y + mode.h / 2.0);
    point(canvas, at, true);
    regions.adopt(&point(canvas, at, false).0).expect("open the named modes");
    let picker = named_control(patch_region(regions), "picker", "Transform").expect("mode picker");
    assert_eq!(picker.get("selected"), Some(&Tree::Text("5".into())));
    let Some(Tree::Items(choices)) = picker.get("choices") else { panic!("missing choices"); };
    let expected = ["pass", "freeze", "band", "shift", "smear", "morph", "pitch"];
    assert_eq!(choices.len(), expected.len());
    for (index, (choice, label)) in choices.iter().zip(expected).enumerate() {
        let Tree::Record(fields) = choice else { panic!("missing choice fields"); };
        assert_eq!(fields.get("label"), Some(&Tree::Text(label.into())));
        assert_eq!(fields.get("key"), Some(&Tree::Text(index.to_string())));
    }
    choose_control(regions, "picker", "Transform", "1");
    choose_menu(regions, "File", "Save");
    let saved = read();
    let settings = saved["tracks"][index]["instrument"]["routing"]["nodes"][2]["settings"].as_array().expect("settings");
    assert_eq!(settings.iter().find(|p| p["key"] == "transform").expect("transform")["value"], 1.0);
    choose_menu(regions, "View", "Patch Controls");
    std::fs::remove_file(copy).expect("remove test project");
}

fn tablepad_patch(regions: &mut Regions, from_signal_flow: bool) -> (i64, Shown) {
    choose_menu(regions, "View", "Bus Routing");
    let canvas = regions.get("root").and_then(|root| scene(&root.node)).expect("Bus Routing canvas");
    let row_y = 235.0 + 1.5 * 37.0;
    let open = if from_signal_flow {
        point(canvas, (680.0, row_y), true);
        point(canvas, (680.0, row_y), false);
        (1074.0, 245.0)
    } else {
        (80.0, row_y)
    };
    point(canvas, open, true);
    let patch = point(canvas, open, false).0;
    regions.adopt(&patch).expect("open tablepad's track patch");
    let patch_canvas = scene(patch_region(regions)).expect("track patch canvas");
    (patch_canvas, patch)
}

fn tablepad_card(regions: &mut Regions, from_signal_flow: bool) -> (i64, (f64, f64), (f64, f64)) {
    let (patch_canvas, patch) = tablepad_patch(regions, from_signal_flow);
    patch.scenes.values().find_map(|scene| {
        let card = scene.access.iter().find(|a| a.label.starts_with("Canvas ") && a.label.ends_with(" 0"))?;
        let outlet = scene.access.iter().find(|a| a.label == "Canvas output 0:0")?;
        Some((patch_canvas, (card.x + card.w / 2.0, card.y + card.h / 2.0), (outlet.x + outlet.w / 2.0, outlet.y + outlet.h / 2.0)))
    }).expect("the patch exposes node zero through its native canvas")
}

/// [PRISM-ROUTING-CABLES] A single LFO fans out to all four distinct wavetable inputs.
fn modulation_sockets(regions: &mut Regions, song: &std::path::Path) {
    let copy = song.with_file_name("modulation.json");
    let mut project: serde_json::Value = serde_json::from_slice(&std::fs::read(song).expect("project")).expect("JSON");
    let track = &mut project["tracks"][1];
    let nodes = track["instrument"]["routing"]["nodes"].as_array_mut().expect("nodes");
    let mut lfo = nodes[1].clone();
    lfo["identity"] = 4.into();
    lfo["component"] = "lfo".into();
    lfo["settings"] = serde_json::json!([]);
    nodes.push(lfo);
    track["positions"].as_array_mut().expect("layout").push(serde_json::json!({"layoutNode":4,"layoutX":610,"layoutY":530}));
    std::fs::write(&copy, serde_json::to_vec(&project).expect("encoded project")).expect("test copy");
    let open = |regions: &mut Regions| {
        choose_menu(regions, "File", "Open Song…");
        let timeline = Library::event(0, &Touch::completed("daw-open", Ok(copy.display().to_string()))).expect("reopen project");
        regions.adopt(&timeline).expect("timeline");
        tablepad_patch(regions, false)
    };
    let (canvas, patch) = open(regions);
    let center = |view: &Shown, label: &str| {
        let a = view.scenes.values().flat_map(|s| &s.access).find(|a| a.label == label).unwrap_or_else(|| panic!("missing {label}"));
        (a.x + a.w / 2.0, a.y + a.h / 2.0)
    };
    let source = center(&patch, "Canvas output 4:0");
    for inlet in 0..4 {
        let destination = center(&patch, &format!("Canvas input 0:{inlet}"));
        point(canvas, source, true);
        point(canvas, destination, true);
        regions.adopt(&point(canvas, destination, false).0).expect("drop cable");
    }
    choose_menu(regions, "File", "Save");
    let (canvas, wired) = open(regions);
    let (_, (x, y), _) = tablepad_card(regions, false);
    point(canvas, (x - 80.0, y - 50.0), true);
    point(canvas, (x - 60.0, y - 40.0), false);
    let (_, moved) = open(regions);
    for inlet in 0..4 {
        let label = format!("Canvas input 0:{inlet}");
        let (x, y) = center(&wired, &label);
        let (mx, my) = center(&moved, &label);
        assert!((mx - x - 20.0).abs() < 0.001 && (my - y - 10.0).abs() < 0.001, "{label} tracks the moved card");
    }
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&copy).expect("saved patch")).expect("project");
    let wires = saved["tracks"][1]["instrument"]["routing"]["cables"].as_array().expect("connections");
    for inlet in 0..4 {
        assert!(wires.contains(&serde_json::json!({"source":4,"outlet":0,"destination":0,"inlet":inlet})), "missing modulation cable to inlet {inlet}");
    }
    std::fs::remove_file(copy).expect("remove test project");
}

fn signal_path_layout(regions: &mut Regions, song: &std::path::Path) {
    choose_menu(regions, "View", "Bus Routing");
    let canvas = regions.get("root").and_then(|root| scene(&root.node)).expect("routing canvas");
    let keys = (80.0, 235.0 + 5.5 * 37.0);
    point(canvas, keys, true);
    let patch = point(canvas, keys, false).0;
    regions.adopt(&patch).expect("open keysL");
    let card = |shown: &Shown, id| {
        let suffix = format!(" {id}");
        let a = shown.scenes.values().flat_map(|s| &s.access)
            .find(|a| a.label.starts_with("Canvas ") && a.label.ends_with(&suffix)).expect("patch card");
        (a.x + a.w / 2.0, a.y + a.h / 2.0, a.w, a.h)
    };
    for id in 1..6 {
        let (left, row, width, _) = card(&patch, id - 1);
        let (right, next_row, next_width, _) = card(&patch, id);
        assert!(left + width / 2.0 < right - next_width / 2.0 && row == next_row,
            "keysL signal path wraps or overlaps between modules {} and {id}", id - 1);
    }
    let canvas = scene(patch_region(regions)).expect("patch canvas");
    let initial = std::fs::read(song).expect("saved project");
    let (original_x, original_y, _, _) = card(&patch, 5);
    let mut moved = patch;
    for _ in 0..3 {
        point(canvas, (950.0, 650.0), true);
        point(canvas, (450.0, 650.0), true);
        moved = point(canvas, (450.0, 650.0), false).0;
    }
    let (x, y, _, _) = card(&moved, 5);
    assert!((150.0..1050.0).contains(&x), "dragging the background reveals the Output at {x}");
    let selected = point(canvas, (x - 55.0, y - 45.0), true).0;
    point(canvas, (x - 55.0, y - 45.0), false);
    let selected_cards: Vec<_> = selected.scenes.values().flat_map(|s| &s.access)
        .filter(|a| a.selected && a.label.starts_with("Canvas ")).map(|a| &a.label).collect();
    assert!(selected_cards.iter().any(|label| label.ends_with(" 5")), "select the panned Output at ({x}, {y}): {selected_cards:?}");
    assert_eq!(std::fs::read(song).expect("viewed project"), initial, "navigating the patch preserves its project");
    point(canvas, (x - 55.0, y - 45.0), true);
    point(canvas, (x - 25.0, y - 25.0), true);
    point(canvas, (x - 25.0, y - 25.0), false);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(song).expect("dragged project")).expect("JSON");
    assert_eq!(saved["tracks"][5]["positions"], serde_json::json!([
        {"layoutNode":5, "layoutX":original_x + 30.0, "layoutY":original_y + 20.0}
    ]), "only the dragged Output gets a position, in project coordinates");
    let (target_x, target_y, _, _) = card(&moved, 4);
    let menu = Library::event(0, &Touch::secondary(canvas, target_x, target_y)).expect("right-click another node");
    let action = menu.scenes.values().flat_map(|s| &s.access).find(|a| a.label == "Delete node").expect("node menu");
    let deleted = point(canvas, (action.x + action.w / 2.0, action.y + action.h / 2.0), true).0;
    assert!(!deleted.scenes.values().flat_map(|s| &s.access).any(|a| a.label.starts_with("Canvas ") && a.label.ends_with(" 4")));
    card(&deleted, 5);
    regions.adopt(&deleted).expect("adopt deletion");
    choose_menu(regions, "Edit", "Undo");
}

fn position_on_release(regions: &mut Regions, song: &std::path::Path) {
    let (canvas, (x, y), outlet) = tablepad_card(regions, false);
    choose_menu(regions, "File", "Save");
    let read = || serde_json::from_slice::<serde_json::Value>(&std::fs::read(song).expect("saved project")).expect("valid project");
    let original = read();
    let off = (-55.0, -45.0);
    let start = (x + off.0, y + off.1);
    let end = (start.0 + if x < 700.0 { 24.0 } else { -24.0 }, start.1 + if y < 500.0 { 18.0 } else { -18.0 });
    point(canvas, start, true);
    point(canvas, end, true);
    assert_eq!(read(), original, "pointer moves leave the saved project alone");
    regions.adopt(&point(canvas, end, false).0).expect("release tablepad node zero");
    // Capture the bytes before switching workspace, saving manually or even opening the chooser.
    let saved = read();
    let mut expected = original;
    let positions = expected["tracks"][1]["positions"].as_array_mut().expect("positions");
    positions.retain(|p| p["layoutNode"] != 0);
    let moved = saved["tracks"][1]["positions"].as_array().expect("saved positions")
        .iter().find(|at| at["layoutNode"] == 0).expect("released node");
    for (axis, target) in [("layoutX", end.0 - off.0), ("layoutY", end.1 - off.1)] {
        let actual = moved[axis].as_f64().expect("numeric canvas coordinate");
        assert!(actual.to_bits().abs_diff(target.to_bits()) <= 1, "{axis}: {actual} versus {target}");
    }
    positions.push(serde_json::json!({"layoutNode":0, "layoutX":moved["layoutX"], "layoutY":moved["layoutY"]}));
    let normalized = |mut project: serde_json::Value| {
        project["tracks"][1]["positions"].as_array_mut().expect("positions").sort_by_key(|p| p["layoutNode"].as_i64());
        project
    };
    assert!(normalized(saved) == normalized(expected), "release persists exactly one layout edit");
    let reopened = song.with_extension("reload.json");
    std::fs::copy(song, &reopened).expect("separate path for a fresh load");
    choose_menu(regions, "File", "Open Song…");
    let loaded = Library::event(0, &Touch::completed("daw-open", Ok(reopened.display().to_string()))).expect("load the saved file");
    regions.adopt(&loaded).expect("fresh project");
    let (_, (restored_x, restored_y), restored_outlet) = tablepad_card(regions, false);
    std::fs::remove_file(reopened).expect("remove the reopened project copy");
    assert!((restored_x - (end.0-off.0)).abs() < 0.001 && (restored_y - (end.1-off.1)).abs() < 0.001);
    assert!((restored_outlet.0 - outlet.0 - (end.0 - start.0)).abs() < 0.001);
    assert!((restored_outlet.1 - outlet.1 - (end.1 - start.1)).abs() < 0.001);
}
/// [DAW-CONTROLS-DIAL] A tap on a canvas dial opens a numeric field for that same parameter.
fn type_into_canvas_dial(regions: &mut Regions, song: &std::path::Path) {
    let (canvas, (x, y), _) = tablepad_card(regions, true);
    let title = (x - 55.0, y - 45.0);
    point(canvas, title, true);
    let selected = point(canvas, title, false).0;
    let (row, dial_scene) = selected.scenes.iter().find(|(_, s)| s.access.iter().any(|a| a.label == "Wavetable 0 Level")).expect("selected module's dials");
    regions.adopt(&selected).expect("select wavetable");
    let dial_canvas = scene_event(patch_region(regions), *row as i64).expect("dial scene event");
    let dial = dial_scene.access.iter().find(|a| a.label == "Wavetable 0 Level").expect("level dial");
    let previous = dial_scene.access.iter().find(|a| a.label == "Wavetable 0 Table position").expect("preceding parameter");
    assert!(dial.h >= 44.0 && dial.y - previous.y >= 52.0, "parameter rows need their full pointer target and gutter");
    assert!(selected.scenes.values().flat_map(|s| &s.shapes).any(|shape| matches!(shape,
        ferroui_osprey_native::scene::Shape::Disc { x, y, radius, .. } if *radius >= 12.0
            && f64::from(*x) > dial.x && f64::from(*x) < dial.x + dial.w
            && (f64::from(*y) - (dial.y + dial.h / 2.0)).abs() < 0.01)), "the dial face grows with its target");
    let at = (dial.x + dial.w / 2.0, dial.y + dial.h / 2.0);
    let original = std::fs::read(song).expect("open project");
    point(dial_canvas, at, true);
    regions.adopt(&point(dial_canvas, at, false).0).expect("tap the level dial");
    choose_control(regions, "field", "Level", "99");
    assert_eq!(std::fs::read(song).expect("project after refused value"), original);
    choose_control(regions, "field", "Level", "0.375");
    choose_menu(regions, "File", "Save");
    let read = || serde_json::from_slice::<serde_json::Value>(&std::fs::read(song).expect("saved project")).expect("project JSON");
    let saved = read();
    let routing = &saved["tracks"][1]["instrument"]["routing"];
    let level = routing["nodes"][0]["settings"].as_array().expect("settings").iter()
        .find(|setting| setting["key"] == "amplitude").expect("wavetable level");
    assert_eq!(level["value"], 0.375);
    choose_menu(regions, "Edit", "Undo");
    assert_eq!(read(), serde_json::from_slice::<serde_json::Value>(&original).expect("original project"));
    choose_menu(regions, "View", "Patch Controls");
    choose_menu(regions, "View", "Timeline");
}

/// [DAW-MIX-DRAG] Offscreen notes must not stall a drag in the visible Timeline inspector.
fn dense_timeline_drag(regions: &mut Regions, song: &std::path::Path, mut project: serde_json::Value) {
    // Eight offscreen parts each play sixteenths over a 128-bar arrangement.
    for track in &mut project["tracks"].as_array_mut().expect("tracks")[TRACKS - 8..] {
        let notes = track["notes"].as_array_mut().expect("part notes");
        let base = notes[0].clone();
        *notes = (0..2048).map(|index| {
            let mut note = base.clone();
            note["start"] = serde_json::json!(f64::from(index) / 4.0);
            note
        }).collect();
    }
    let dense = song.with_file_name("dense.json");
    std::fs::write(&dense, serde_json::to_vec(&project).expect("encode dense song")).expect("write dense song");
    choose_menu(regions, "File", "Open Song…");
    let opened = Library::event(0, &Touch::completed("daw-open", Ok(dense.display().to_string()))).expect("open dense song");
    assert!(shows_label(&opened, "Arrangement"), "the dense project opens on its Timeline");
    regions.adopt(&opened).expect("draw dense timeline");
    let canvas = regions.get("root").and_then(|root| scene(&root.node)).expect("timeline canvas");
    point(canvas, (1160.0, 348.0), true);
    let mut elapsed = 0.0;
    for step in 1..=24 {
        let x = 1160.0 + f64::from(step) * 9.0;
        let (shown, ms) = point(canvas, (x, 348.0), true);
        let gain = (x - 1154.0) * 4.0 / 262.0;
        assert!(shows_label(&shown, &format!("{gain:.2}×")), "the timeline inspector follows the drag");
        elapsed += ms;
    }
    let (_, release) = point(canvas, (1376.0, 348.0), false);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&dense).expect("released file")).expect("project");
    assert_eq!(saved["tracks"][0]["gain"], 3.39);
    let mean = elapsed / 24.0;
    println!("dense timeline drag: {mean:.2} ms mean over 24 moves with 16,384 offscreen notes; release and save {release:.2} ms");
    assert!(mean < 1000.0 / 120.0, "dragging a timeline control averaged {mean:.2} ms per move");
    std::fs::remove_file(dense).expect("remove dense song");
}

#[test]
fn tapping_timeline_lanes_answers_every_tap_within_one_display_frame() {
    assert_eq!(Library::init(), Ok(()), "the daw's top level must run");
    // A copy: a Mix edit saves into the project it opened, and the tracked song must stay as it is.
    let folder = std::env::temp_dir().join(format!("osprey-daw-events-{}", std::process::id()));
    let song = folder.join("song.json");
    assert!(std::fs::create_dir_all(&folder).is_ok(), "the test keeps its song in {}", folder.display());
    let copied = std::fs::copy(repo("osprey/examples/daw/tracks/codex-version.json"), &song);
    assert!(copied.is_ok(), "the song is copied: {copied:?}");
    let arguments = vec![
        "--machine".to_owned(),
        repo("osprey/packages/signal_flow/signal_flow.machine.json"),
        "--song".to_owned(),
        song.display().to_string(),
    ];
    assert_eq!(Library::arguments(&arguments), Ok(()), "the daw takes its machine and song");
    let opened = Library::open(0, 0, "macos", "/tmp", true);
    let Ok(first) = opened else {
        panic!("the daw must draw its first frame: {opened:?}");
    };
    assert!(shapes(&first) >= TIMELINE_SHAPES, "the Timeline draws {} shapes", shapes(&first));
    let mut regions = Regions::new();
    assert_eq!(regions.adopt(&first), Ok(()), "the first frame tells its regions");
    let Some(canvas) = regions.get("root").and_then(|root| scene(&root.node)) else {
        panic!("the Timeline must draw a canvas");
    };

    // [DAW-HEADER-TIMING] Edit the header through native controls and verify the persisted project.
    let timing = point(canvas, (900.0, 105.0), true).0;
    regions.adopt(&timing).expect("open BPM editor");
    choose_control(&mut regions, "field", "BPM", "96.5");
    choose_control(&mut regions, "field", "Time signature", "6/8");
    choose_control(&mut regions, "button", "Apply timing", "");
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&song).expect("timing file")).expect("timing JSON");
    assert_eq!(saved["tempo"], 96.5);
    assert_eq!((saved["meterTop"].as_i64(), saved["meterBottom"].as_i64()), (Some(6), Some(8)));
    choose_menu(&mut regions, "Edit", "Undo");
    signal_path_layout(&mut regions, &song);
    position_on_release(&mut regions, &song);
    modulation_sockets(&mut regions, &song);
    spectral_picker(&mut regions, &song);
    choose_menu(&mut regions, "File", "Open Song…");
    let reset = Library::event(0, &Touch::completed("daw-open", Ok(song.display().to_string()))).expect("return to the test project");
    regions.adopt(&reset).expect("restore the timeline");
    menu_bar(&mut regions);
    type_into_canvas_dial(&mut regions, &song);
    create_group(&mut regions, canvas, &song);
    drag_piano_note(&mut regions, canvas, &song);

    let mut costs = Vec::new();
    let mut drawn = std::collections::BTreeSet::new();
    for round in 0..3 {
        for track in 0..TRACKS {
            let top = (track / 7 * 7).min(TRACKS - 7);
            if track % 7 == 0 {
                let rail = (1121.0, 221.0 + (top as f64 + 3.5) * 217.0 / TRACKS as f64);
                point(canvas, rail, true);
                point(canvas, rail, false);
            }
            let at = lane(track - top, (track * 5 + round * 13) as f64 % 40.0 / 40.0);
            let (pressed, down) = point(canvas, at, true);
            assert!(shapes(&pressed) >= TIMELINE_SHAPES, "track {track} keeps the Timeline drawn");
            assert!(pressed.diagnostics().next().is_none(), "the tap on track {track} is not refused");
            let (released, up) = point(canvas, at, false);
            assert!(shapes(&released) >= TIMELINE_SHAPES, "releasing keeps the Timeline drawn");
            drawn.insert(shapes(&released));
            costs.push(down);
            costs.push(up);
        }
    }

    // [DAW-TRANSPORT-PLAYHEAD] Ruler taps move the playhead; the lanes stay drawn.
    for bar in 0..10 {
        let at = (198.0 + f64::from(bar) * 89.4, 207.0);
        let (pressed, down) = point(canvas, at, true);
        assert!(pressed.diagnostics().next().is_none(), "the ruler tap at bar step {bar} is not refused");
        let (released, up) = point(canvas, at, false);
        assert!(shapes(&released) >= TIMELINE_SHAPES, "moving the playhead keeps the Timeline drawn");
        costs.push(down);
        costs.push(up);
    }

    // The Mix tab, then every track's gain fader pressed, dragged and released (rows start at y 235,
    // 37 units each).
    point(canvas, MIX_TAB, true);
    let (mix, _) = point(canvas, MIX_TAB, false);
    assert!(shapes(&mix) >= MIX_SHAPES, "the Mix page draws {} shapes", shapes(&mix));
    output_routing(&mut regions, canvas, &song);
    let read_mix = || serde_json::from_slice::<serde_json::Value>(&std::fs::read(&song).expect("mixed project")).expect("project JSON");
    let before_drag = read_mix();
    let mut drags = Vec::new();
    for track in 0..TRACKS {
        let y = 235.0 + (track as f64 + 0.5) * 37.0;
        for step in 0..=6 {
            let (moved, cost) = point(canvas, (216.0 + f64::from(step) * 8.0, y), true);
            assert!(shapes(&moved) >= MIX_SHAPES, "dragging track {track}'s gain keeps the Mix drawn");
            assert!(moved.diagnostics().next().is_none(), "the drag on track {track} is not refused");
            let gain = 4.0 * (1.0 + f64::from(step) * 8.0) / 55.0;
            assert!(shows_label(&moved, &format!("{gain:.2}×")), "track {track} gain is drawn at this pointer position before release");
            drags.push(cost);
        }
        if track == 0 { assert_eq!(read_mix(), before_drag, "pointer moves are provisional"); }
        let (released, cost) = point(canvas, (264.0, y), false);
        assert!(shapes(&released) >= MIX_SHAPES, "releasing track {track}'s gain keeps the Mix drawn");
        drags.push(cost);
        if track == 0 {
            regions.adopt(&released).expect("release updates Undo");
            let after_drag = read_mix();
            assert!(after_drag["tracks"][0]["gain"].as_f64().expect("released gain") > 2.0);
            choose_menu(&mut regions, "Edit", "Undo");
            assert_eq!(read_mix(), before_drag, "one Undo restores the entire fader gesture");
            choose_menu(&mut regions, "Edit", "Redo");
            assert_eq!(read_mix(), after_drag);
        }
    }
    let rendered = read_mix();
    let tracks = rendered["tracks"].as_array().expect("project tracks");
    assert_eq!(tracks.len(), TRACKS);
    for track in tracks {
        assert!(track["gain"].as_f64().expect("track gain") > 2.0, "the fader changes the gain of {}", track["name"]);
    }
    let mean = drags.iter().sum::<f64>() / drags.len() as f64;
    let worst = drags.iter().copied().fold(0.0, f64::max);
    println!("mix fader drags: mean {mean:.2} ms, worst {worst:.2} ms over {} events", drags.len());
    assert!(mean < FRAME_MS, "a Mix fader event takes {mean:.2} ms on average; one frame is {FRAME_MS:.2} ms");
    assert!(worst < 2.0 * FRAME_MS, "the slowest Mix fader event takes {worst:.2} ms; two frames are {:.2} ms", 2.0 * FRAME_MS);

    // Each track's piano roll draws its own notes, so taps that select tracks redraw different scenes.
    assert!(drawn.len() >= 8, "the taps selected tracks: {} distinct scenes", drawn.len());
    let measured = &costs[WARM_UP..];
    let mean = measured.iter().sum::<f64>() / measured.len() as f64;
    let worst = measured.iter().copied().fold(0.0, f64::max);
    println!("timeline taps: mean {mean:.2} ms, worst {worst:.2} ms over {} events", measured.len());
    assert!(mean < FRAME_MS, "a Timeline tap takes {mean:.2} ms on average; one frame is {FRAME_MS:.2} ms");
    assert!(worst < 2.0 * FRAME_MS, "the slowest Timeline tap takes {worst:.2} ms; two frames are {:.2} ms", 2.0 * FRAME_MS);
    dense_timeline_drag(&mut regions, &song, rendered);
    std::fs::remove_file(&song).expect("remove this run's project");
    std::fs::remove_dir(&folder).expect("remove this run's folder");
}
