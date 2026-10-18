use serde::{Deserialize, Serialize};

/// JIS kuten code (`plane-ku-ten`, e.g. `1-48-19`).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Kuten {
    pub plane: u8,
    pub ku: u8,
    pub ten: u8,
}

impl Kuten {
    /// Parse `plane-ku-ten` (ku/ten 1..=99, plane 1..=2).
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('-');
        let plane: u8 = parts.next()?.parse().ok()?;
        let ku: u8 = parts.next()?.parse().ok()?;
        let ten: u8 = parts.next()?.parse().ok()?;
        if parts.next().is_some() || !(1..=2).contains(&plane) {
            return None;
        }
        if ku == 0 || ku > 99 || ten == 0 || ten > 99 {
            return None;
        }
        Some(Self { plane, ku, ten })
    }
}

/// O'Neill index: number plus optional suffix letter (`2078A`).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Oneill {
    pub num: u32,
    pub suffix: Option<char>,
}

impl Oneill {
    /// Parse digits with an optional trailing ASCII uppercase letter.
    pub fn parse(text: &str) -> Option<Self> {
        let (num_text, suffix) = match text.strip_suffix(|c: char| c.is_ascii_uppercase()) {
            Some(num) => (num, text.chars().last()),
            None => (text, None),
        };
        if num_text.is_empty() || !num_text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Self {
            num: num_text.parse().ok()?,
            suffix,
        })
    }
}

/// SKIP code (`n-nn-nn`): pattern plus two counts whose meaning
/// depends on the pattern (part strokes for 1-3; see KANJIDIC docs).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Skip {
    pub pattern: u8,
    pub first: u8,
    pub second: u8,
}

impl Skip {
    /// Parse `pattern-first-second` (all stroke-count scale).
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('-');
        let pattern: u8 = parts.next()?.parse().ok()?;
        let first: u8 = parts.next()?.parse().ok()?;
        let second: u8 = parts.next()?.parse().ok()?;
        if parts.next().is_some() || !(1..=4).contains(&pattern) {
            return None;
        }
        Some(Self {
            pattern,
            first,
            second,
        })
    }
}

/// Four-corner code (`TLTRBLBR.extra`, each a single 0-9 digit).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FourCorner {
    pub top_left: u8,
    pub top_right: u8,
    pub bottom_left: u8,
    pub bottom_right: u8,
    pub extra: u8,
}

impl FourCorner {
    /// Parse `DDDD.D` (five single digits).
    pub fn parse(text: &str) -> Option<Self> {
        let (corners, extra) = text.split_once('.')?;
        let mut digits = corners.bytes();
        let digit = |it: &mut std::str::Bytes| -> Option<u8> {
            let b = it.next()?;
            b.is_ascii_digit().then_some(b - b'0')
        };
        let code = Self {
            top_left: digit(&mut digits)?,
            top_right: digit(&mut digits)?,
            bottom_left: digit(&mut digits)?,
            bottom_right: digit(&mut digits)?,
            extra: extra.parse().ok()?,
        };
        if digits.next().is_some() || code.extra > 9 {
            return None;
        }
        Some(code)
    }
}

/// De Roo code: plain number lookup key.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Deroo(pub u32);

impl Deroo {
    /// Parse a non-empty digit string.
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Self(text.parse().ok()?))
    }
}

/// Nelson index: plain number lookup key.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Nelson(pub u32);

impl Nelson {
    /// Parse a non-empty digit string.
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Self(text.parse().ok()?))
    }
}

/// NJECD index: plain number lookup key.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Njecd(pub u32);

impl Njecd {
    /// Parse a non-empty digit string.
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Self(text.parse().ok()?))
    }
}

/// Spahn/Hadamitzky descriptor (`nxnn.n`, e.g. `3k11.2`: strokes in
/// the identifying radical, radical letter, other strokes, sequence
/// number).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ShDesc {
    pub radical_strokes: u32,
    pub radical: char,
    pub other_strokes: u32,
    pub index: u32,
}

impl ShDesc {
    /// Parse `nxnn.n` (single lowercase radical letter).
    pub fn parse(text: &str) -> Option<Self> {
        let (head, index) = text.split_once('.')?;
        let index: u32 = index.parse().ok()?;
        let letter_at = head.find(|c: char| c.is_ascii_lowercase())?;
        let (radical_strokes, rest) = head.split_at(letter_at);
        let mut rest_chars = rest.chars();
        let radical = rest_chars.next()?;
        let other_strokes: &str = rest_chars.as_str();
        if radical_strokes.is_empty()
            || !radical_strokes.bytes().all(|b| b.is_ascii_digit())
            || other_strokes.is_empty()
            || !other_strokes.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        Some(Self {
            radical_strokes: radical_strokes.parse().ok()?,
            radical,
            other_strokes: other_strokes.parse().ok()?,
            index,
        })
    }
}

impl std::fmt::Display for Kuten {
    /// `plane-ku-ten`, as kanjidic2 carries it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}-{}", self.plane, self.ku, self.ten)
    }
}

impl std::fmt::Display for Oneill {
    /// Digits plus optional suffix letter.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.suffix {
            Some(s) => write!(f, "{}{}", self.num, s),
            None => write!(f, "{}", self.num),
        }
    }
}

impl std::fmt::Display for Skip {
    /// `pattern-first-second`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}-{}", self.pattern, self.first, self.second)
    }
}

impl std::fmt::Display for FourCorner {
    /// `TLTRBLBR.extra`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}{}.{}",
            self.top_left, self.top_right, self.bottom_left, self.bottom_right, self.extra
        )
    }
}

impl std::fmt::Display for Deroo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for Nelson {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for Njecd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for ShDesc {
    /// `nxnn.n`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}.{}",
            self.radical_strokes, self.radical, self.other_strokes, self.index
        )
    }
}
