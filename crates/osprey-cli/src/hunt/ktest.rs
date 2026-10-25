//! KLEE's `.ktest` files [SYMEX-FINDINGS]: the concrete bytes of every
//! symbolic object on one explored path, big-endian framed
//! (klee/lib/Basic/KTest.cpp, versions 3 and 4).

/// One path's concrete inputs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Ktest {
    pub(crate) objects: Vec<Object>,
}

/// A symbolic object by the name the mock libc gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Object {
    pub(crate) name: String,
    pub(crate) bytes: Vec<u8>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], String> {
        let end = self
            .at
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len());
        let end = end.ok_or_else(|| format!("ktest truncated at byte {}", self.at))?;
        let slice = self.bytes.get(self.at..end).unwrap_or_default();
        self.at = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let raw: [u8; 4] = self.take(4)?.try_into().unwrap_or_default();
        Ok(u32::from_be_bytes(raw))
    }

    fn length(&mut self) -> Result<usize, String> {
        let n = self.u32()?;
        usize::try_from(n).map_err(|error| format!("ktest length does not fit this host: {error}"))
    }
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Ktest, String> {
    let mut cursor = Cursor { bytes, at: 0 };
    let magic = cursor.take(5)?;
    if magic != b"KTEST" && magic != b"BOUT\n" {
        return Err("not a KTEST file".to_string());
    }
    let version = cursor.u32()?;
    if version > 4 {
        return Err(format!("KTEST version {version} is newer than this tool"));
    }
    for _ in 0..cursor.length()? {
        let length = cursor.length()?;
        let _argument = cursor.take(length)?;
    }
    if version >= 2 {
        let _sym_argvs = cursor.u32()?;
        let _sym_argv_len = cursor.u32()?;
    }
    let count = cursor.length()?;
    let objects = (0..count)
        .map(|_| object(&mut cursor, version))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Ktest { objects })
}

fn object(cursor: &mut Cursor<'_>, version: u32) -> Result<Object, String> {
    let name_length = cursor.length()?;
    let name = String::from_utf8_lossy(cursor.take(name_length)?).into_owned();
    let size = cursor.length()?;
    let bytes = cursor.take(size)?.to_vec();
    if version >= 4 {
        for _ in 0..cursor.length()? {
            let _pointer = (cursor.u32()?, cursor.u32()?, cursor.u32()?);
        }
    }
    Ok(Object { name, bytes })
}

impl Ktest {
    pub(crate) fn object(&self, name: &str) -> Option<&[u8]> {
        self.objects
            .iter()
            .find(|object| object.name == name)
            .map(|object| object.bytes.as_slice())
    }

    /// The stream `name` cut to its symbolic length `<name>.len`, a host
    /// `int` KLEE stores little-endian [SYMEX-MOCK-STREAM].
    pub(crate) fn stream(&self, name: &str) -> Option<Vec<u8>> {
        let bytes = self.object(name)?;
        let length = self
            .object(&format!("{name}.len"))
            .and_then(host_int)
            .unwrap_or(bytes.len());
        Some(bytes.get(..length).unwrap_or(bytes).to_vec())
    }

    /// Every opened file's bytes by path: the objects that are not stdin, a
    /// length, an open flag, or an entropy, clock or errno draw.
    pub(crate) fn files(&self) -> Vec<(String, Vec<u8>)> {
        self.objects
            .iter()
            .filter(|object| self.is_file_stream(&object.name))
            .filter_map(|object| {
                self.stream(&object.name)
                    .map(|bytes| (object.name.clone(), bytes))
            })
            .collect()
    }

    fn is_file_stream(&self, name: &str) -> bool {
        is_file_stream(name)
            || (self.object(&format!("{name}.ok")).and_then(host_int) == Some(1)
                && self
                    .object(&format!("{name}.len"))
                    .and_then(host_int)
                    .is_some())
    }
}

fn is_file_stream(name: &str) -> bool {
    let (_, suffix) = name.rsplit_once('.').unwrap_or((name, ""));
    name != "stdin"
        && suffix != "len"
        && suffix != "ok"
        && !["entropy.", "clock.", "errno."]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

fn host_int(bytes: &[u8]) -> Option<usize> {
    let raw: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    usize::try_from(i32::from_le_bytes(raw)).ok()
}

/// A version-3 KTEST holding `objects`, for fixtures.
#[cfg(test)]
pub(crate) fn encode(objects: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = b"KTEST".to_vec();
    let length = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).to_be_bytes();
    out.extend(3u32.to_be_bytes());
    out.extend(length(1));
    out.extend(length(4));
    out.extend(b"prog");
    out.extend(0u32.to_be_bytes());
    out.extend(0u32.to_be_bytes());
    out.extend(length(objects.len()));
    for (name, bytes) in objects {
        out.extend(length(name.len()));
        out.extend(name.as_bytes());
        out.extend(length(bytes.len()));
        out.extend(*bytes);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_objects_and_cuts_streams_to_their_length() {
        let bytes = encode(&[
            ("stdin", b"9\nxxxxxx"),
            ("stdin.len", &2i32.to_le_bytes()),
            ("data.txt.ok", &[1, 0, 0, 0]),
            ("data.txt", b"payload!"),
            ("data.txt.len", &7i32.to_le_bytes()),
            ("entropy.0", &[0; 8]),
        ]);
        let ktest = parse(&bytes).expect("ktest");
        assert_eq!(ktest.objects.len(), 6);
        assert_eq!(ktest.stream("stdin"), Some(b"9\n".to_vec()));
        assert_eq!(
            ktest.files(),
            vec![("data.txt".to_string(), b"payload".to_vec())]
        );
        assert_eq!(ktest.stream("missing"), None);
    }

    fn file_with_metadata_name(name: &str) -> Ktest {
        let length = format!("{name}.len");
        let opened = format!("{name}.ok");
        parse(&encode(&[
            (name, b"9trailing"),
            (&length, &1i32.to_le_bytes()),
            (&opened, &1i32.to_le_bytes()),
        ]))
        .expect("valid file stream")
    }

    #[test]
    fn real_files_with_metadata_like_names_keep_their_replay_bytes() {
        for name in [
            "input.len",
            "input.ok",
            "entropy.data",
            "clock.data",
            "errno.data",
        ] {
            let ktest = file_with_metadata_name(name);
            assert_eq!(ktest.stream(name), Some(b"9".to_vec()), "{name}");
            assert_eq!(
                ktest.files(),
                vec![(name.to_string(), b"9".to_vec())],
                "{name}"
            );
        }
    }

    #[test]
    fn a_stream_without_a_length_is_whole() {
        let ktest = parse(&encode(&[("stdin", b"abc")])).expect("ktest");
        assert_eq!(ktest.stream("stdin"), Some(b"abc".to_vec()));
    }

    #[test]
    fn version_four_pointer_records_are_skipped() {
        let mut bytes = b"KTEST".to_vec();
        bytes.extend(4u32.to_be_bytes());
        bytes.extend(0u32.to_be_bytes());
        bytes.extend(0u32.to_be_bytes());
        bytes.extend(0u32.to_be_bytes());
        bytes.extend(1u32.to_be_bytes());
        bytes.extend(1u32.to_be_bytes());
        bytes.push(b'p');
        bytes.extend(2u32.to_be_bytes());
        bytes.extend(b"hi");
        bytes.extend(1u32.to_be_bytes());
        bytes.extend([0u8; 12]);
        let ktest = parse(&bytes).expect("ktest v4");
        assert_eq!(ktest.object("p"), Some(&b"hi"[..]));
    }

    #[test]
    fn malformed_files_are_errors_not_panics() {
        assert!(parse(b"KTES").is_err_and(|e| e.contains("truncated")));
        assert!(parse(b"NOPE!\0\0\0\x03").is_err_and(|e| e.contains("not a KTEST")));
        let mut newer = b"KTEST".to_vec();
        newer.extend(9u32.to_be_bytes());
        assert!(parse(&newer).is_err_and(|e| e.contains("version 9")));
        let mut cut = encode(&[("stdin", b"abcdef")]);
        cut.truncate(cut.len() - 3);
        assert!(parse(&cut).is_err());
    }
}
