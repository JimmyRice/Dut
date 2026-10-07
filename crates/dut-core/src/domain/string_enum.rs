/// Declares an enum whose variants each go by a fixed piece of text, such as
/// a line's `TKL` or a file's name on the MTR portal.
///
/// The text is written once, beside the variant, and everything that depends
/// on it is generated from that one list: the accessor, `Display`, and, with
/// `unknown`, `ALL` and a case-insensitive `FromStr`. Hand-written, `ALL`
/// is the one list the compiler does not check against the variants, so a
/// variant left out of it is silently never parsed.
///
/// ```ignore
/// string_enum! {
///     /// Doc comments and derives go on the enum as usual.
///     #[derive(Clone, Copy, Debug, Eq, PartialEq)]
///     pub enum Colour {
///         /// Variants may carry their own docs.
///         Red => "red",
///         Green => "green",
///     }
///
///     /// The accessor's docs say where the text appears.
///     pub const fn name;
///     unknown = UnknownColour;
/// }
/// ```
///
/// `unknown` names an existing unit error type, returned when no variant's
/// text matches. Leave it out for an enum that is only ever written, never
/// read back, and it gets no `ALL` or `FromStr`.
macro_rules! string_enum {
    (@parse $vis:vis $name:ident, $accessor:ident, [], [$($variant:ident),+]) => {};
    (@parse $vis:vis $name:ident, $accessor:ident, [$error:ident], [$($variant:ident),+]) => {
        impl $name {
            /// Every variant, in declaration order.
            $vis const ALL: [Self; { [$(Self::$variant),+].len() }] = [$(Self::$variant),+];
        }

        #[doc = concat!(
            "Parses the text [`", stringify!($accessor), "`](Self::", stringify!($accessor),
            ") returns, ignoring ASCII case."
        )]
        impl ::core::str::FromStr for $name {
            type Err = $error;

            fn from_str(input: &str) -> Result<Self, Self::Err> {
                Self::ALL
                    .into_iter()
                    .find(|variant| variant.$accessor().eq_ignore_ascii_case(input))
                    .ok_or($error)
            }
        }
    };
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($(#[$variant_meta:meta])* $variant:ident => $text:literal),+ $(,)?
        }

        $(#[$accessor_meta:meta])*
        $accessor_vis:vis const fn $accessor:ident;
        $(unknown = $error:ident;)?
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $($(#[$variant_meta])* $variant),+
        }

        impl $name {
            $(#[$accessor_meta])*
            $accessor_vis const fn $accessor(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str(self.$accessor())
            }
        }

        string_enum!(@parse $vis $name, $accessor, [$($error)?], [$($variant),+]);
    };
}

pub(crate) use string_enum;

#[cfg(test)]
mod tests {
    use thiserror::Error;

    #[derive(Debug, Eq, Error, PartialEq)]
    #[error("unknown shade")]
    struct UnknownShade;

    string_enum! {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Shade {
            Light => "light",
            /// Documented variants keep their docs.
            Dark => "DARK",
        }

        /// The shade's name.
        const fn name;
        unknown = UnknownShade;
    }

    string_enum! {
        #[derive(Clone, Copy, Debug)]
        enum Write {
            Only => "only",
        }

        const fn text;
    }

    #[test]
    fn lists_every_variant_in_declaration_order() {
        assert_eq!(Shade::ALL, [Shade::Light, Shade::Dark]);
    }

    #[test]
    fn displays_and_parses_the_text_ignoring_case() {
        assert_eq!(Shade::Dark.to_string(), "DARK");
        assert_eq!("dark".parse(), Ok(Shade::Dark));
        assert_eq!("LIGHT".parse(), Ok(Shade::Light));
    }

    #[test]
    fn rejects_text_no_variant_has() {
        for input in ["", "dar", "darker", " dark"] {
            assert_eq!(input.parse::<Shade>(), Err(UnknownShade), "{input:?}");
        }
    }

    #[test]
    fn works_without_parsing() {
        assert_eq!(Write::Only.text(), "only");
        assert_eq!(Write::Only.to_string(), "only");
    }
}
