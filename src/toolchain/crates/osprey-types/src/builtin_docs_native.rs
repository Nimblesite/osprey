//! Shared-frame writer prose ([NATIVE-SHM-VIEW], [NATIVE-SHM-SCENE], [NATIVE-SHM-ASK]);
//! signatures live in the canonical builtin table.
use crate::builtin_docs::BuiltinDoc;
use crate::builtin_docs_lang::builtin_doc;

const VIEW_EXAMPLE: &str = "let row = nativeViewRow(2, -1, -1, \"Hello\", 255, 0, 14.0, 4.0, 0.0, 0.0, 400)\nlet fitted = nativeViewExtend(row, 120.0, 0.0, 0.0, 0, 0, \"\")\nprint(\"row=${row}; fitted=${fitted}\")";

pub(crate) static NATIVE: &[BuiltinDoc] = &[
    builtin_doc!(
        "nativeViewRow",
        "Appends one control and its text to the view frame the host seals. Returns the row for its children to name as `parent`, or -1 once the view is refused.",
        ["kind" => "The control kind", "parent" => "The parent row, -1 for the root", "action" => "The action ordinal, -1 for none", "words" => "The control's text", "ink" => "Text colour as 0xRRGGBBAA", "fill" => "Background colour as 0xRRGGBBAA", "points" => "Text size", "inset" => "Padding", "gap" => "Space between children", "corner" => "Corner radius", "heft" => "Font weight from 100 to 900"],
        VIEW_EXAMPLE,
    ),
    builtin_doc!(
        "nativeViewExtend",
        "Gives the row it names fixed extents, a flex share, state flags, a border colour and a second text. Returns -1 once the view is refused.",
        ["row" => "The row nativeViewRow returned", "wide" => "Fixed width, 0 for natural", "tall" => "Fixed height, 0 for natural", "share" => "Flex share", "traits" => "State flags", "edge" => "Border colour as 0xRRGGBBAA", "hint" => "The second text"],
        VIEW_EXAMPLE,
    ),
    builtin_doc!(
        "nativeSceneShape",
        "Appends one scene row and its text to the frame the host seals. Returns nonzero once the scene is refused.",
        ["code" => "The scene row code", "index" => "The row's index", "x" => "Left edge", "y" => "Top edge", "width" => "Width", "height" => "Height", "angle" => "Rotation", "tint" => "Colour as 0xRRGGBBAA", "layer" => "Drawing depth", "words" => "The row's text"],
        "print(nativeSceneShape(0, 0, 0.0, 0.0, 10.0, 10.0, 0.0, 255, 0, \"\"))",
    ),
    builtin_doc!(
        "nativeAsk",
        "Appends one ask field and its text, which the host answers as a capability request. Returns nonzero once the frame is refused.",
        ["field" => "The ask field code; below 0 refuses the frame", "number" => "The field's number", "words" => "The field's text"],
        "print(nativeAsk(1, 0, \"https://example.com\"))",
    ),
];
