//! The numeric types, described once. Implements [TYPE-NUMERIC-SIZED]
//! (docs/specs/language/0004-TypeSystem.md): a kind and a width name each of
//! the ten types, and every rule about a number — its LLVM type, its device
//! spelling, its container tag — is a column of this one table rather than a
//! branch per type somewhere else.

use crate::ty::Type;

/// How a numeric type's bits are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NumericKind {
    /// Two's-complement signed integer.
    Signed,
    /// Unsigned integer.
    Unsigned,
    /// IEEE 754 binary floating point.
    Binary,
}

/// One numeric type: its kind and its width in bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Numeric {
    /// How the bits are read.
    pub kind: NumericKind,
    /// The width in bits.
    pub bits: u8,
}

impl Numeric {
    /// `int8`.
    pub const INT8: Numeric = Numeric::new(NumericKind::Signed, 8);
    /// `int16`.
    pub const INT16: Numeric = Numeric::new(NumericKind::Signed, 16);
    /// `int32`.
    pub const INT32: Numeric = Numeric::new(NumericKind::Signed, 32);
    /// `int`, the default integer type.
    pub const INT: Numeric = Numeric::new(NumericKind::Signed, 64);
    /// `uint8`.
    pub const UINT8: Numeric = Numeric::new(NumericKind::Unsigned, 8);
    /// `uint16`.
    pub const UINT16: Numeric = Numeric::new(NumericKind::Unsigned, 16);
    /// `uint32`.
    pub const UINT32: Numeric = Numeric::new(NumericKind::Unsigned, 32);
    /// `float16`, IEEE 754 binary16.
    pub const FLOAT16: Numeric = Numeric::new(NumericKind::Binary, 16);
    /// `float32`, IEEE 754 binary32.
    pub const FLOAT32: Numeric = Numeric::new(NumericKind::Binary, 32);
    /// `float`, IEEE 754 binary64, the default float type.
    pub const FLOAT: Numeric = Numeric::new(NumericKind::Binary, 64);

    /// Every numeric type, in the order the specification's table lists them.
    pub const ALL: [Numeric; 10] = [
        Numeric::INT8,
        Numeric::INT16,
        Numeric::INT32,
        Numeric::INT,
        Numeric::UINT8,
        Numeric::UINT16,
        Numeric::UINT32,
        Numeric::FLOAT16,
        Numeric::FLOAT32,
        Numeric::FLOAT,
    ];

    const fn new(kind: NumericKind, bits: u8) -> Numeric {
        Numeric { kind, bits }
    }

    /// The numeric type a primitive spelling names, if it names one.
    #[must_use]
    pub fn of_name(name: &str) -> Option<Numeric> {
        Numeric::ALL.into_iter().find(|n| n.name() == name)
    }

    /// The numeric type a nullary constructor is, if it is one.
    #[must_use]
    pub fn of_type(ty: &Type) -> Option<Numeric> {
        match ty {
            Type::Con { name, args } if args.is_empty() => Numeric::of_name(name),
            _ => None,
        }
    }

    /// The type's spelling in source and diagnostics.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match (self.kind, self.bits) {
            (NumericKind::Signed, 8) => "int8",
            (NumericKind::Signed, 16) => "int16",
            (NumericKind::Signed, 32) => "int32",
            (NumericKind::Unsigned, 8) => "uint8",
            (NumericKind::Unsigned, 16) => "uint16",
            (NumericKind::Unsigned, 32) => "uint32",
            (NumericKind::Binary, 16) => "float16",
            (NumericKind::Binary, 32) => "float32",
            (NumericKind::Binary, 64) => "float",
            _ => "int",
        }
    }

    /// The LLVM type a value travels as. `float16` travels as its bits
    /// ([TYPE-NUMERIC-SIZED]: no `half`, whose helpers the wasm32 lane lacks).
    #[must_use]
    pub const fn llvm(self) -> &'static str {
        match (self.kind, self.bits) {
            (NumericKind::Binary, 32) => "float",
            (NumericKind::Binary, 64) => "double",
            (_, 8) => "i8",
            (_, 16) => "i16",
            (_, 32) => "i32",
            _ => "i64",
        }
    }

    /// The element spelling in a container's owner tag (`List#u8`,
    /// `Gpu#float`), distinct for every row where the LLVM type is not
    /// ([TYPE-NUMERIC-WORD]).
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match (self.kind, self.bits) {
            (NumericKind::Unsigned, 8) => "u8",
            (NumericKind::Unsigned, 16) => "u16",
            (NumericKind::Unsigned, 32) => "u32",
            (NumericKind::Binary, 16) => "f16",
            _ => self.llvm(),
        }
    }

    /// The device dialect's spelling of an element of this type
    /// ([GPU-BACKEND-DEVICE]): binary64 is the bits of a software double.
    #[must_use]
    pub const fn device(self) -> &'static str {
        match (self.kind, self.bits) {
            (NumericKind::Signed, 8) => "char",
            (NumericKind::Signed, 16) => "short",
            (NumericKind::Signed, 32) => "int",
            (NumericKind::Signed, _) => "long",
            (NumericKind::Unsigned, 8) => "uchar",
            (NumericKind::Unsigned, 16) => "ushort",
            (NumericKind::Unsigned, _) => "uint",
            (NumericKind::Binary, 16) => "half",
            (NumericKind::Binary, 32) => "float",
            (NumericKind::Binary, _) => "ulong",
        }
    }

    /// The type a conversion builtin is named for: `toInt`, `toInt8`, `toUint32`,
    /// `toFloat`, `toFloat16` ([BUILTIN-CONVERT-INT], [BUILTIN-CONVERT-FLOAT]).
    #[must_use]
    pub fn conversion_target(builtin: &str) -> Option<Numeric> {
        let written = builtin.strip_prefix("to")?;
        Numeric::ALL.into_iter().find(|n| {
            n.name().eq_ignore_ascii_case(written) && written.starts_with(char::is_uppercase)
        })
    }

    /// The byte width of one element of this type [GPU-BUFFER-ELEM]
    /// (docs/specs/runtime/0034-GPUComputation.md): the table's own column,
    /// so a dense buffer's element size has no second spelling anywhere.
    #[must_use]
    pub const fn bytes(self) -> u8 {
        self.bits / 8
    }

    /// The inclusive range of an integer type; `None` for a float.
    #[must_use]
    pub const fn range(self) -> Option<(i128, i128)> {
        match self.kind {
            NumericKind::Binary => None,
            NumericKind::Signed => Some((
                -(1_i128 << (self.bits - 1)),
                (1_i128 << (self.bits - 1)) - 1,
            )),
            NumericKind::Unsigned => Some((0, (1_i128 << self.bits) - 1)),
        }
    }

    /// Whether an integer value is inside this type's range.
    #[must_use]
    pub const fn fits(self, value: i128) -> bool {
        match self.range() {
            Some((low, high)) => low <= value && value <= high,
            None => true,
        }
    }

    /// Neither of the two default types: the same-type rule has no exception
    /// for it ([TYPE-NUMERIC-SAME-TYPE]).
    #[must_use]
    pub fn is_sized(self) -> bool {
        self != Numeric::INT && self != Numeric::FLOAT
    }

    /// The width a float operation computes at: `float16` through binary32
    /// with one final rounding, every other float its own width
    /// [TYPE-NUMERIC-ARITH-WIDTH].
    #[must_use]
    pub const fn float_compute(self) -> Numeric {
        if self.is_float() && self.bits == 16 {
            Numeric::FLOAT32
        } else {
            self
        }
    }

    /// An IEEE float of any width.
    #[must_use]
    pub const fn is_float(self) -> bool {
        matches!(self.kind, NumericKind::Binary)
    }

    /// A signed or unsigned integer of any width.
    #[must_use]
    pub const fn is_integer(self) -> bool {
        !self.is_float()
    }

    /// A two's-complement integer: widens by sign extension.
    #[must_use]
    pub const fn is_signed(self) -> bool {
        matches!(self.kind, NumericKind::Signed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_round_trips_through_its_name_and_has_a_distinct_tag() {
        let mut tags = std::collections::BTreeSet::new();
        for n in Numeric::ALL {
            assert_eq!(Numeric::of_name(n.name()), Some(n));
            assert!(tags.insert(n.tag()), "{} shares a tag", n.name());
            assert_eq!(Numeric::of_type(&Type::prim(n.name())), Some(n));
        }
        assert_eq!(Numeric::of_name("Int"), None);
        assert_eq!(Numeric::of_type(&Type::list(Type::int())), None);
    }

    #[test]
    fn the_default_types_keep_their_spellings() {
        assert_eq!(Numeric::INT.name(), "int");
        assert_eq!(Numeric::INT.llvm(), "i64");
        assert_eq!(Numeric::INT.device(), "long");
        assert_eq!(Numeric::FLOAT.name(), "float");
        assert_eq!(Numeric::FLOAT.llvm(), "double");
        assert_eq!(Numeric::FLOAT.device(), "ulong");
        assert_eq!(Numeric::FLOAT16.llvm(), "i16");
        assert_eq!(Numeric::FLOAT16.tag(), "f16");
        assert_eq!(Numeric::UINT8.bytes(), 1);
        assert_eq!(Numeric::FLOAT16.bytes(), 2);
        assert_eq!(Numeric::UINT32.bytes(), 4);
        assert_eq!(Numeric::INT.bytes(), 8);
        assert!(Numeric::UINT32.is_integer() && Numeric::FLOAT32.is_float());
    }
}
