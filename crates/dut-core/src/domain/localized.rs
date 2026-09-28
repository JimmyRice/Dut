/// A value published in both of the MTR's customer-facing languages.
///
/// The type is generic so it can hold static reference data
/// (`Localized<&'static str>`) as well as owned upstream text
/// (`Localized<String>`) or richer records.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Localized<T> {
    /// English.
    pub en: T,
    /// Traditional Chinese.
    pub tc: T,
}

impl<T> Localized<T> {
    pub const fn new(en: T, tc: T) -> Self {
        Self { en, tc }
    }
}
