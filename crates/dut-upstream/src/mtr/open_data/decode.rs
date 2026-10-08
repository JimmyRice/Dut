//! Reading the portal's CSV files, and the text fixes every file needs.

use std::{error::Error, str::FromStr};

use csv::{ReaderBuilder, StringRecord, Trim};
use serde::de::DeserializeOwned;
use tracing::error;

use dut_core::domain::{localized::Localized, reference::SourceFile};

use crate::mtr::open_data::error::OpenDataError;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Decodes every row of a CSV file into `T`, matching columns by header
/// name so that reordered columns still decode.
///
/// A leading byte order mark is ignored, fields are trimmed, and rows whose
/// fields are all empty, which some files end with, are skipped. A row that
/// does not decode fails the whole file.
pub(super) fn decode_rows<T: DeserializeOwned>(
    file: SourceFile,
    body: &[u8],
) -> Result<Vec<T>, OpenDataError> {
    let body = body.strip_prefix(UTF8_BOM).unwrap_or(body);
    let mut reader = ReaderBuilder::new().trim(Trim::All).from_reader(body);
    let headers = reader
        .headers()
        .cloned()
        .map_err(|cause| csv_error(file, cause))?;

    let mut rows = Vec::new();
    let mut record = StringRecord::new();
    while reader
        .read_record(&mut record)
        .map_err(|cause| csv_error(file, cause))?
    {
        if is_blank(&record) {
            continue;
        }
        let row = record
            .deserialize(Some(&headers))
            .map_err(|cause| csv_error(file, cause))?;
        rows.push(row);
    }
    Ok(rows)
}

/// Parses a field into a domain type, naming the file, column, and value
/// when it cannot.
pub(super) fn parse<T>(
    file: SourceFile,
    column: &'static str,
    value: &str,
) -> Result<T, OpenDataError>
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
{
    value.parse().map_err(|cause| {
        error!(%file, column, value, error = &cause as &dyn Error, "open data has an invalid value");
        OpenDataError::InvalidValue {
            file,
            column,
            value: value.to_owned(),
            cause: Box::new(cause),
        }
    })
}

/// Bilingual text as riders should read it; see [`clean_text`].
pub(super) fn clean_localized(en: &str, tc: &str) -> Localized<String> {
    Localized::new(clean_text(en), clean_text(tc))
}

/// Fixes the text defects the portal's files carry:
///
/// - HTML character references, such as `&#32171;` for 綫, are decoded.
/// - 茘 (U+8318), a variant the station list uses in 荔景 and 荔枝角, becomes
///   the standard 荔 (U+8354) that station signs and every other MTR feed use.
/// - Runs of spaces become one, each line is trimmed, and blank lines are
///   dropped, keeping line breaks as `\n`.
pub(super) fn clean_text(text: &str) -> String {
    let decoded = decode_character_references(text).replace('\u{8318}', "\u{8354}");
    decoded
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Decodes decimal (`&#32171;`) and hexadecimal (`&#x7DAB;`) character
/// references, leaving anything else, including invalid references, as is.
fn decode_character_references(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("&#") {
        decoded.push_str(before);
        match referenced_char(after) {
            Some((character, remainder)) => {
                decoded.push(character);
                rest = remainder;
            }
            None => {
                decoded.push_str("&#");
                rest = after;
            }
        }
    }
    decoded.push_str(rest);
    decoded
}

/// Given the text after a reference's `&#`, the character it stands for and
/// the text after its `;`.
fn referenced_char(text: &str) -> Option<(char, &str)> {
    let (digits, rest) = text.split_once(';')?;
    let (digits, radix) = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => (hex, 16),
        None => (digits, 10),
    };
    if digits.is_empty() || !digits.chars().all(|digit| digit.is_digit(radix)) {
        return None;
    }
    let code = u32::from_str_radix(digits, radix).ok()?;
    Some((char::from_u32(code)?, rest))
}

fn is_blank(record: &StringRecord) -> bool {
    record.iter().all(str::is_empty)
}

fn csv_error(file: SourceFile, cause: csv::Error) -> OpenDataError {
    let line = cause.position().map(csv::Position::line);
    error!(%file, line, error = &cause as &dyn Error, "open data is not the CSV expected");
    OpenDataError::Csv { file, cause }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Row {
        #[serde(rename = "Code")]
        code: String,
        #[serde(rename = "Name")]
        name: String,
    }

    #[test]
    fn skips_the_byte_order_mark_and_blank_rows() {
        let body = "\u{feff}\"Code\",\"Name\"\r\n\"ADM\",\" Admiralty \"\r\n,\r\n,\r\n";

        let rows: Vec<Row> =
            decode_rows(SourceFile::LinesAndStations, body.as_bytes()).expect("rows decode");

        assert_eq!(
            rows,
            [Row {
                code: "ADM".to_owned(),
                name: "Admiralty".to_owned()
            }]
        );
    }

    #[test]
    fn matches_columns_by_name() {
        let body = "Name,Code\nAdmiralty,ADM\n";

        let rows: Vec<Row> =
            decode_rows(SourceFile::LinesAndStations, body.as_bytes()).expect("rows decode");

        assert_eq!(rows[0].code, "ADM");
    }

    #[test]
    fn fails_when_a_column_is_missing() {
        let body = "Code\nADM\n";

        let result = decode_rows::<Row>(SourceFile::LinesAndStations, body.as_bytes());

        assert!(matches!(result, Err(OpenDataError::Csv { .. })));
    }

    #[test]
    fn decodes_character_references() {
        assert_eq!(clean_text("閃燈路&#32171;圖"), "閃燈路綫圖");
        assert_eq!(clean_text("&#x7DAB;"), "綫");
        assert_eq!(
            clean_text("A &# B &#99999999; &#; &#+12;"),
            "A &# B &#99999999; &#; &#+12;"
        );
    }

    #[test]
    fn uses_the_standard_form_of_lai() {
        assert_eq!(clean_text("茘枝角"), "荔枝角");
    }

    #[test]
    fn tidies_spacing_but_keeps_line_breaks() {
        assert_eq!(clean_text(" B、C  和 D 出口 "), "B、C 和 D 出口");
        assert_eq!(
            clean_text("Arrival Hall\r\nDeparture Hall\n"),
            "Arrival Hall\nDeparture Hall"
        );
    }
}
