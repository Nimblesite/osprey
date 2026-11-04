// Shared real Osprey SDK fixture used by the renderer/browser contracts.
export const SDK_COMPONENT_SOURCE = String.raw`
namespace sdk_renderer_probe
import ui::Ui
import ui::Editors
import ui::Maths
import ui::Encode
import ui::Doc
import ui::Runtime
import ui::Keyed
import ui::Messages
import vscode::Webview

type Act = Changed | Gone

controls expanded = [
    Ui::toggle ("SDK switch", true, Changed),
    Ui::picker ("SDK choice", [Ui::choice ("a", "Alpha"), Ui::choice ("b", "Beta")], "a", Changed),
    Ui::disclosure ("SDK details", expanded, Changed, [Ui::button ("Nested action", "plus", Changed)]),
    ui::Field(label = "SDK notes", value = "one\ntwo", prompt = "Notes", secure = false, multiline = true, commit = ui::OnSubmit, bind = Changed),
    ui::Field(label = "SDK secret", value = "private", prompt = "Password", secure = true, multiline = false, commit = ui::OnSubmit, bind = Changed),
    ui::Button(label = "SDK disabled", symbol = "lock", role = ui::Destructive, enabled = false, action = Changed),
    ui::Field(label = "SDK draft", value = "", prompt = "Draft", secure = false, multiline = false, commit = ui::OnEdit, bind = Changed),
]

proof () = [
    Editors::editor ("SDK argument", "p ∧ q\nFrom hp.", 0, [Editors::mark (2, true, ui::Danger, "Needs attention", "The goal is q; this step establishes p.")], Changed, Changed),
    Maths::typeset (ui::Frac (ui::Sup (ui::Sym "x") (ui::Num "2")) (ui::Root (ui::Sym "y")), "x squared over square root of y"),
    Ui::evidence ("SDK evidence", "37 lines"),
    Ui::pair ("SDK pair", "42 nodes"),
    Ui::label ("SDK label", "info"),
    ui::Spacer,
    ui::Divider,
    ui::Segments(tones = [ui::Good, ui::Caution, ui::Danger], label = "SDK health"),
    Ui::empty ("tray", "SDK empty", "No items exist"),
    ui::Progress(label = "SDK progress", detail = "Compiling"),
    ui::Swatch(name = "SDK palette", light = "#112233", dark = "#ddeeff"),
    ui::Brand,
]

drawing expanded = Ui::screen ("SDK vocabulary", "Compiled from Osprey", [], [Ui::layer [Ui::section ("SDK proof", "SDK footer", listConcat (controls expanded) (proof ())) ]])

variants () = Ui::aside (Ui::column [
    Ui::section ("", "", []),
    Ui::toggle ("SDK off", false, Changed),
    ui::Swatch(name = "SDK invalid palette", light = "red", dark = "url(https://example.invalid)"),
    ui::Progress(label = "SDK progress", detail = "Safe values"),
    Ui::slot "root",
], ui::Leading, Ui::rows ([Ui::item ("plain", Ui::text "<untrusted>")], "plain", Changed), 9999, Changed)

tree mode = match mode
    "variants" => variants ()
    _ => drawing (mode == "expanded")

(** A real SDK application that canonicalizes input and can withdraw its field. *)
module Normalized
    slice () = perform Signal<string>.read ()
    drawing () =
        value = perform Signal<string>.read ()
        match value == "removed"
            true => Ui::text "SDK removed"
            false => Ui::column [Ui::field ("SDK normalized", value, "Normalized", Changed), Ui::button ("Hide field", "xmark", Gone)]
    regions (before, value, frames, _) =
        handle static Signal<string>
            read => value
        changed = signalChanges before value (\text => JsonString text) (signalDependencies slice)
        [Keyed::region (frames, "root", changed, drawing)]
    initial _ = Runtime::step ""
    update (_, current, message) = Messages::route (message, \_ => Runtime::step "removed", \(_, _) => Runtime::step "CANONICAL", \(_, _) => Runtime::step current, \(_, _) => Runtime::step current, \(_, _) => Runtime::step current, \_ => Runtime::step current, \(_, _) => Runtime::step current, \(_, _) => Runtime::step current, \_ => Runtime::step current, \(_, _) => Runtime::step current)
    save value = JsonString value
    restore doc = jsonText doc ?: ""
    palette = ui::Theme(tokens = [], spacing = [4], feedback = 160, settle = 220)
    (** Start the field application through the framework. *)
    export start context = Runtime::start (context, initial, regions, save, palette)
    (** Deliver a typed event through the framework. *)
    export dispatch (saved, event) = Runtime::dispatch (saved, event, update, regions, save, restore)

main () =
    source = input ()
    match Doc::has (Doc::parse source, "operation")
        true => print (Webview::respond (source, Normalized::start, Normalized::dispatch))
        false => print (Doc::write (listGet (Encode::walk ([], tree source)).nodes 0 ?: JsonNull))
`;
