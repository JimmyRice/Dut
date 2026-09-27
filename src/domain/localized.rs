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

    /// Borrows both translations.
    pub const fn as_ref(&self) -> Localized<&T> {
        Localized {
            en: &self.en,
            tc: &self.tc,
        }
    }

    /// Transforms both translations with the same function.
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Localized<U> {
        Localized {
            en: f(self.en),
            tc: f(self.tc),
        }
    }
}
