use jiff::{Timestamp, civil::DateTime};

use dut_core::domain::time::HONG_KONG;

/// Parses the `yyyy-MM-dd HH:mm:ss` Hong Kong local times used by MTR feeds.
pub(super) fn parse_local(input: &str) -> Result<Timestamp, jiff::Error> {
    let local = DateTime::strptime("%Y-%m-%d %H:%M:%S", input)?;
    HONG_KONG.to_timestamp(local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interprets_times_as_hong_kong_local_time() {
        let timestamp = parse_local("2026-09-27 22:35:36").expect("time should parse");

        assert_eq!(timestamp.to_string(), "2026-09-27T14:35:36Z");
    }

    #[test]
    fn rejects_other_formats() {
        assert!(parse_local("2026-09-27T22:35:36").is_err());
        assert!(parse_local("").is_err());
    }
}
