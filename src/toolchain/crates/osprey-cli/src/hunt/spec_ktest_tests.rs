//! KLEE's `.ktest` framing under every version, magic, truncation and
//! corruption, and the stream and file objects the mock libc names
//! [SYMEX-FINDINGS] [SYMEX-MOCK-STREAM] [SYMEX-MOCK-STATELESS].

#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing element is a test failure, not a production panic"
)]

use super::ktest;
use super::spec_tests::permutations;

// ---- [SYMEX-MOCK-STREAM]: ktest framing -------------------------------------

/// A KTEST file of any supported `version` with `arguments`, for framing
/// permutations; version 4 adds an empty pointer table per object.
fn framed(magic: &[u8], version: u32, arguments: &[&str], objects: &[(&str, &[u8])]) -> Vec<u8> {
    let be = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).to_be_bytes();
    let mut out = magic.to_vec();
    out.extend(version.to_be_bytes());
    out.extend(be(arguments.len()));
    for argument in arguments {
        out.extend(be(argument.len()));
        out.extend(argument.as_bytes());
    }
    if version >= 2 {
        out.extend([0u8; 8]);
    }
    out.extend(be(objects.len()));
    for (name, bytes) in objects {
        out.extend(be(name.len()));
        out.extend(name.as_bytes());
        out.extend(be(bytes.len()));
        out.extend(*bytes);
        if version >= 4 {
            out.extend(0u32.to_be_bytes());
        }
    }
    out
}

const OBJECTS: [(&str, &[u8]); 3] = [
    ("stdin", b"9\nzz"),
    ("stdin.len", &[2, 0, 0, 0]),
    ("in.txt", b""),
];

#[test]
fn every_magic_version_and_argument_list_parses_the_same_objects() {
    for magic in [&b"KTEST"[..], &b"BOUT\n"[..]] {
        for version in 0..=4 {
            for arguments in [
                &[][..],
                &["prog"][..],
                &["prog", "--sym-stdin", "64", ""][..],
            ] {
                let parsed = ktest::parse(&framed(magic, version, arguments, &OBJECTS));
                assert!(parsed.is_ok(), "v{version} {arguments:?}: {parsed:?}");
                let parsed = parsed.unwrap_or_default();
                let names: Vec<&str> = parsed.objects.iter().map(|o| o.name.as_str()).collect();
                assert_eq!(names, ["stdin", "stdin.len", "in.txt"]);
                assert_eq!(parsed.stream("stdin"), Some(b"9\n".to_vec()));
                assert_eq!(parsed.object("in.txt"), Some(&b""[..]));
            }
        }
    }
}

#[test]
fn a_newer_version_is_refused_by_number() {
    for version in [5, 6, 42, u32::MAX] {
        let error = ktest::parse(&framed(b"KTEST", version, &[], &OBJECTS)).expect_err("newer");
        assert!(error.contains(&format!("version {version}")), "{error}");
    }
}

/// Every strict prefix of a valid file is an error, never a panic or a
/// partial result.
#[test]
fn every_truncation_of_every_version_is_an_error() {
    for version in [1, 3, 4] {
        let bytes = framed(b"KTEST", version, &["prog"], &OBJECTS);
        assert!(ktest::parse(&bytes).is_ok());
        for cut in 0..bytes.len() {
            assert!(
                ktest::parse(&bytes[..cut]).is_err(),
                "v{version} cut at {cut}"
            );
        }
    }
}

/// One wrong byte anywhere in the magic is "not a KTEST file".
#[test]
fn every_single_byte_corruption_of_the_magic_is_refused() {
    let good = ktest::encode(&OBJECTS);
    for (at, original) in good.iter().copied().enumerate().take(5) {
        for byte in (0..=u8::MAX).filter(|byte| *byte != original) {
            let mut bad = good.clone();
            bad[at] = byte;
            let error = ktest::parse(&bad).expect_err("bad magic");
            assert_eq!(error, "not a KTEST file", "byte {at} = {byte}");
        }
    }
}

/// Lengths a hostile or corrupt file claims are bounds-checked, not trusted.
#[test]
fn huge_claimed_lengths_fail_without_allocating() {
    let max = u32::MAX.to_be_bytes();
    let mut arguments = b"KTEST".to_vec();
    arguments.extend(3u32.to_be_bytes());
    arguments.extend(max);
    let mut objects = b"KTEST".to_vec();
    objects.extend(3u32.to_be_bytes());
    objects.extend([0u8; 12]);
    objects.extend(max);
    let mut name = objects.clone();
    name.truncate(name.len() - 4);
    name.extend(1u32.to_be_bytes());
    name.extend(max);
    let mut size = name.clone();
    size.truncate(size.len() - 4);
    size.extend(1u32.to_be_bytes());
    size.push(b'x');
    size.extend(max);
    for bytes in [arguments, objects, name, size] {
        assert!(
            ktest::parse(&bytes).is_err_and(|e| e.contains("truncated")),
            "{bytes:?}"
        );
    }
}

/// `(length object, what `stream` returns)` over `abcdef`: the length is a
/// little-endian host `int`; one that is negative, short or past the buffer
/// leaves the stream whole.
#[test]
fn every_kind_of_stream_length_cuts_or_keeps_the_stream() {
    let cases: [(&[u8], &[u8]); 10] = [
        (&[0, 0, 0, 0], b""),
        (&[3, 0, 0, 0], b"abc"),
        (&[6, 0, 0, 0], b"abcdef"),
        (&[7, 0, 0, 0], b"abcdef"),
        (&[0xff, 0xff, 0xff, 0xff], b"abcdef"),
        (&[0xff, 0xff, 0xff, 0x7f], b"abcdef"),
        (&[0, 0, 0, 3], b"abcdef"),
        (&[2, 0], b"abcdef"),
        (&[], b"abcdef"),
        (&[3, 0, 0, 0, 9], b"abc"),
    ];
    for (length, expected) in cases {
        for name in ["stdin", "data.txt", "dir/sub/a.csv"] {
            let len_name = format!("{name}.len");
            let objects: [(&str, &[u8]); 2] = [(name, b"abcdef"), (len_name.as_str(), length)];
            let parsed = ktest::parse(&ktest::encode(&objects)).expect("ktest");
            assert_eq!(
                parsed.stream(name).as_deref(),
                Some(expected),
                "{name} {length:?}"
            );
        }
    }
}

/// `(object name, is it an opened file)`: stdin, lengths, open flags and the
/// entropy, clock and errno draws are not files [SYMEX-MOCK-STATELESS].
const FILE_NAMES: [(&str, bool); 18] = [
    ("stdin", false),
    ("stdin.len", false),
    ("data.txt", true),
    ("data.txt.len", false),
    ("data.txt.ok", false),
    ("entropy.0", false),
    ("entropy.17", false),
    ("clock.0", false),
    ("clock.3", false),
    ("errno.1", false),
    ("config", true),
    ("dir/sub/a.csv", true),
    ("a.len.txt", true),
    ("a.ok.json", true),
    ("entropy", true),
    ("clockwork.txt", true),
    ("stdin.txt", true),
    ("../up.txt", true),
];

#[test]
fn only_opened_files_are_files_whatever_the_object_order() {
    let objects: Vec<(&str, &[u8])> = FILE_NAMES
        .iter()
        .map(|(name, _)| (*name, &b"xy"[..]))
        .collect();
    let expected: Vec<(String, Vec<u8>)> = FILE_NAMES
        .iter()
        .filter(|(_, file)| *file)
        .map(|(name, _)| ((*name).to_string(), b"xy".to_vec()))
        .collect();
    let parsed = ktest::parse(&ktest::encode(&objects)).expect("ktest");
    assert_eq!(parsed.files(), expected);
    let mut reversed = objects.clone();
    reversed.reverse();
    let mut backwards = ktest::parse(&ktest::encode(&reversed))
        .expect("ktest")
        .files();
    backwards.reverse();
    assert_eq!(backwards, expected);
}

/// A stream and its length in any order: the same bytes.
#[test]
fn a_stream_and_its_length_pair_up_in_any_order() {
    let objects: [(&str, &[u8]); 5] = [
        ("stdin", b"hello"),
        ("stdin.len", &[4, 0, 0, 0]),
        ("in.txt", b"payload"),
        ("in.txt.len", &[3, 0, 0, 0]),
        ("in.txt.ok", &[1, 0, 0, 0]),
    ];
    for order in permutations(&objects) {
        let parsed = ktest::parse(&ktest::encode(&order)).expect("ktest");
        assert_eq!(parsed.stream("stdin"), Some(b"hell".to_vec()));
        assert_eq!(
            parsed.files(),
            vec![("in.txt".to_string(), b"pay".to_vec())]
        );
    }
}

#[test]
fn the_first_object_of_a_repeated_name_wins() {
    let parsed =
        ktest::parse(&ktest::encode(&[("stdin", b"one"), ("stdin", b"two")])).expect("ktest");
    assert_eq!(parsed.object("stdin"), Some(&b"one"[..]));
    assert_eq!(parsed.objects.len(), 2);
}

#[test]
fn every_byte_value_survives_the_round_trip() {
    let all: Vec<u8> = (0..=u8::MAX).collect();
    let parsed = ktest::parse(&ktest::encode(&[("stdin", all.as_slice())])).expect("ktest");
    assert_eq!(parsed.stream("stdin"), Some(all));
}
