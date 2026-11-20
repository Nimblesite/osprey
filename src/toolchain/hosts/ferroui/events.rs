//! Host events as the typed call the application exports ([NATIVE-SHM-EVENT]): a kind, the control
//! it names, two texts, two numbers and two flags. Nothing is encoded; what a control means is the
//! application's, through the binding its number names.

/// One host event, as `osprey_app_event` takes it after the host clock.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Touch {
    pub kind: i64,
    pub ordinal: i64,
    pub words: String,
    pub detail: String,
    pub x: f64,
    pub y: f64,
    pub down: bool,
    pub shift: bool,
}

/// The event kinds `ui::Hosting` declares.
pub mod kind {
    pub const CONTROL: i64 = 0;
    pub const KEY: i64 = 1;
    pub const POINTER: i64 = 2;
    pub const VIEWED: i64 = 3;
    // 4 to 7 are a board's stroke and lasso, which this host does not draw.
    pub const COMPLETED: i64 = 8;
    pub const PHASE: i64 = 9;
    pub const FRAME: i64 = 10;
    pub const APPEARANCE: i64 = 11;
}

impl Touch {
    fn of(kind: i64) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    /// The control numbered `ordinal` delivering `words`: empty for a tap, `true`/`false` for a
    /// switch, a key for a picker, text for a field, a whole number for a splitter or caret.
    pub fn control(ordinal: i64, words: &str) -> Self {
        Self {
            ordinal,
            words: words.to_owned(),
            ..Self::of(kind::CONTROL)
        }
    }

    /// A key going down or up over the scene numbered `ordinal`.
    pub fn key(ordinal: i64, name: &str, down: bool) -> Self {
        Self {
            ordinal,
            words: name.to_owned(),
            down,
            ..Self::of(kind::KEY)
        }
    }

    /// The pointer over the scene numbered `ordinal`, in scene units.
    pub fn pointer(ordinal: i64, x: f64, y: f64, down: bool, shift: bool) -> Self {
        Self {
            ordinal,
            x,
            y,
            down,
            shift,
            ..Self::of(kind::POINTER)
        }
    }

    /// The scene numbered `ordinal` measured in scene units.
    pub fn viewed(ordinal: i64, width: f64, height: f64) -> Self {
        Self {
            ordinal,
            x: width,
            y: height,
            ..Self::of(kind::VIEWED)
        }
    }

    /// The capability under `ticket` finished: its result text, or the reason it failed.
    pub fn completed(ticket: &str, outcome: Result<String, String>) -> Self {
        let (down, detail) = match outcome {
            Ok(text) => (true, text),
            Err(reason) => (false, reason),
        };
        Self {
            words: ticket.to_owned(),
            detail,
            down,
            ..Self::of(kind::COMPLETED)
        }
    }

    /// A lifecycle phase, such as `active`, `closed` or `open:<path>`.
    pub fn phase(name: &str) -> Self {
        Self {
            words: name.to_owned(),
            ..Self::of(kind::PHASE)
        }
    }

    /// One display frame, `elapsed` seconds after the last.
    pub fn frame(elapsed: f64) -> Self {
        Self {
            x: elapsed,
            ..Self::of(kind::FRAME)
        }
    }

    /// The system appearance changed.
    pub fn appearance(dark: bool) -> Self {
        Self {
            down: dark,
            ..Self::of(kind::APPEARANCE)
        }
    }
}

/// A URL's path with its percent escapes decoded, so `My%20Song.mp3` is the file's name.
pub fn unescaped(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = bytes
            .get(i + 1..i + 3)
            .and_then(|hex| std::str::from_utf8(hex).ok());
        match (
            bytes[i],
            escaped.and_then(|hex| u8::from_str_radix(hex, 16).ok()),
        ) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
