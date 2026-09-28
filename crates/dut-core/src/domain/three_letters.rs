/// Exactly three ASCII letters, stored inline and uppercased, so a code built
/// on it is `Copy` and cheap to hash.
///
/// Station codes and Light Rail stop codes share this shape but are
/// assigned independently, so each wraps it in its own type.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ThreeLetters([u8; 3]);

impl ThreeLetters {
    /// Uppercases `bytes`, or returns `None` unless they are three ASCII
    /// letters.
    pub(crate) const fn parse(bytes: &[u8]) -> Option<Self> {
        let [first, second, third] = bytes else {
            return None;
        };
        let letters = [
            first.to_ascii_uppercase(),
            second.to_ascii_uppercase(),
            third.to_ascii_uppercase(),
        ];
        if letters[0].is_ascii_uppercase()
            && letters[1].is_ascii_uppercase()
            && letters[2].is_ascii_uppercase()
        {
            Some(Self(letters))
        } else {
            None
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        // `parse` only admits ASCII letters, so this never falls back.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uppercases_three_letters() {
        let letters = ThreeLetters::parse(b"tKo").map(|letters| letters.as_str().to_owned());

        assert_eq!(letters.as_deref(), Some("TKO"));
    }

    #[test]
    fn rejects_anything_else() {
        for input in ["", "TK", "TKOO", "T1O", "將軍澳"] {
            assert!(ThreeLetters::parse(input.as_bytes()).is_none(), "{input}");
        }
    }
}
