use std::{fmt, str::FromStr};

use thiserror::Error;

use crate::domain::localized::Localized;

/// A three-letter MTR station code such as `TKO`.
///
/// The code is stored inline as uppercase ASCII, so it is `Copy`, cheap to
/// hash, and safe to use as a cache key. A syntactically valid code is not
/// necessarily a known station; use [`Station::find`] for that.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StationCode([u8; 3]);

impl StationCode {
    /// Builds a code from a literal in a static table.
    ///
    /// Only call this in `const` items: an invalid literal then fails the
    /// build instead of panicking at runtime.
    pub const fn from_static(code: &str) -> Self {
        match Self::from_ascii(code.as_bytes()) {
            Some(code) => code,
            None => panic!("station code literals must be three ASCII letters"),
        }
    }

    /// Returns the code as an uppercase string slice.
    pub fn as_str(&self) -> &str {
        // The constructor only admits ASCII letters, so this never falls back.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    const fn from_ascii(bytes: &[u8]) -> Option<Self> {
        let [first, second, third] = bytes else {
            return None;
        };
        let code = [
            first.to_ascii_uppercase(),
            second.to_ascii_uppercase(),
            third.to_ascii_uppercase(),
        ];
        if code[0].is_ascii_uppercase()
            && code[1].is_ascii_uppercase()
            && code[2].is_ascii_uppercase()
        {
            Some(Self(code))
        } else {
            None
        }
    }
}

/// Parses a station code case-insensitively.
impl FromStr for StationCode {
    type Err = InvalidStationCode;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::from_ascii(input.as_bytes()).ok_or(InvalidStationCode)
    }
}

impl fmt::Display for StationCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Debug for StationCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "StationCode({})", self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("a station code must be three ASCII letters")]
pub struct InvalidStationCode;

/// A station served by the Next Train API, with its bilingual name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Station {
    pub code: StationCode,
    pub name: Localized<&'static str>,
}

impl Station {
    /// Looks up a known station by its code.
    pub fn find(code: StationCode) -> Option<&'static Self> {
        STATIONS
            .binary_search_by_key(&code, |station| station.code)
            .ok()
            .and_then(|index| STATIONS.get(index))
    }

    /// Every known station, ordered by code.
    #[cfg(test)]
    pub fn all() -> &'static [Self] {
        STATIONS
    }

    const fn new(code: &str, en: &'static str, tc: &'static str) -> Self {
        Self {
            code: StationCode::from_static(code),
            name: Localized::new(en, tc),
        }
    }
}

/// Stations listed in the Next Train API data dictionary (v1.7), sorted by
/// code so [`Station::find`] can binary search. English names follow the data dictionary, with its "Price Edward"
/// typo corrected.
const STATIONS: &[Station] = &[
    Station::new("ADM", "Admiralty", "金鐘"),
    Station::new("AIR", "Airport", "機場"),
    Station::new("AUS", "Austin", "柯士甸"),
    Station::new("AWE", "AsiaWorld-Expo", "博覽館"),
    Station::new("CAB", "Causeway Bay", "銅鑼灣"),
    Station::new("CEN", "Central", "中環"),
    Station::new("CHH", "Choi Hung", "彩虹"),
    Station::new("CHW", "Chai Wan", "柴灣"),
    Station::new("CIO", "City One", "第一城"),
    Station::new("CKT", "Che Kung Temple", "車公廟"),
    Station::new("CSW", "Cheung Sha Wan", "長沙灣"),
    Station::new("DIH", "Diamond Hill", "鑽石山"),
    Station::new("DIS", "Disneyland Resort", "迪士尼"),
    Station::new("ETS", "East Tsim Sha Tsui", "尖東"),
    Station::new("EXC", "Exhibition Centre", "會展"),
    Station::new("FAN", "Fanling", "粉嶺"),
    Station::new("FOH", "Fortress Hill", "炮台山"),
    Station::new("FOT", "Fo Tan", "火炭"),
    Station::new("HAH", "Hang Hau", "坑口"),
    Station::new("HEO", "Heng On", "恆安"),
    Station::new("HFC", "Heng Fa Chuen", "杏花邨"),
    Station::new("HIK", "Hin Keng", "顯徑"),
    Station::new("HKU", "HKU", "香港大學"),
    Station::new("HOK", "Hong Kong", "香港"),
    Station::new("HOM", "Ho Man Tin", "何文田"),
    Station::new("HUH", "Hung Hom", "紅磡"),
    Station::new("JOR", "Jordan", "佐敦"),
    Station::new("KAT", "Kai Tak", "啟德"),
    Station::new("KET", "Kennedy Town", "堅尼地城"),
    Station::new("KOB", "Kowloon Bay", "九龍灣"),
    Station::new("KOT", "Kowloon Tong", "九龍塘"),
    Station::new("KOW", "Kowloon", "九龍"),
    Station::new("KSR", "Kam Sheung Road", "錦上路"),
    Station::new("KWF", "Kwai Fong", "葵芳"),
    Station::new("KWH", "Kwai Hing", "葵興"),
    Station::new("KWT", "Kwun Tong", "觀塘"),
    Station::new("LAK", "Lai King", "荔景"),
    Station::new("LAT", "Lam Tin", "藍田"),
    Station::new("LCK", "Lai Chi Kok", "荔枝角"),
    Station::new("LET", "Lei Tung", "利東"),
    Station::new("LHP", "LOHAS Park", "康城"),
    Station::new("LMC", "Lok Ma Chau", "落馬洲"),
    Station::new("LOF", "Lok Fu", "樂富"),
    Station::new("LOP", "Long Ping", "朗屏"),
    Station::new("LOW", "Lo Wu", "羅湖"),
    Station::new("MEF", "Mei Foo", "美孚"),
    Station::new("MKK", "Mong Kok East", "旺角東"),
    Station::new("MOK", "Mong Kok", "旺角"),
    Station::new("MOS", "Ma On Shan", "馬鞍山"),
    Station::new("NAC", "Nam Cheong", "南昌"),
    Station::new("NOP", "North Point", "北角"),
    Station::new("NTK", "Ngau Tau Kok", "牛頭角"),
    Station::new("OCP", "Ocean Park", "海洋公園"),
    Station::new("OLY", "Olympic", "奧運"),
    Station::new("POA", "Po Lam", "寶琳"),
    Station::new("PRE", "Prince Edward", "太子"),
    Station::new("QUB", "Quarry Bay", "鰂魚涌"),
    Station::new("RAC", "Racecourse", "馬場"),
    Station::new("SHM", "Shek Mun", "石門"),
    Station::new("SHS", "Sheung Shui", "上水"),
    Station::new("SHT", "Sha Tin", "沙田"),
    Station::new("SHW", "Sheung Wan", "上環"),
    Station::new("SIH", "Siu Hong", "兆康"),
    Station::new("SKM", "Shek Kip Mei", "石硤尾"),
    Station::new("SKW", "Shau Kei Wan", "筲箕灣"),
    Station::new("SOH", "South Horizons", "海怡半島"),
    Station::new("SSP", "Sham Shui Po", "深水埗"),
    Station::new("STW", "Sha Tin Wai", "沙田圍"),
    Station::new("SUN", "Sunny Bay", "欣澳"),
    Station::new("SUW", "Sung Wong Toi", "宋皇臺"),
    Station::new("SWH", "Sai Wan Ho", "西灣河"),
    Station::new("SYP", "Sai Ying Pun", "西營盤"),
    Station::new("TAK", "Tai Koo", "太古"),
    Station::new("TAP", "Tai Po Market", "大埔墟"),
    Station::new("TAW", "Tai Wai", "大圍"),
    Station::new("TIH", "Tin Hau", "天后"),
    Station::new("TIK", "Tiu Keng Leng", "調景嶺"),
    Station::new("TIS", "Tin Shui Wai", "天水圍"),
    Station::new("TKO", "Tseung Kwan O", "將軍澳"),
    Station::new("TKW", "To Kwa Wan", "土瓜灣"),
    Station::new("TSH", "Tai Shui Hang", "大水坑"),
    Station::new("TST", "Tsim Sha Tsui", "尖沙咀"),
    Station::new("TSW", "Tsuen Wan", "荃灣"),
    Station::new("TSY", "Tsing Yi", "青衣"),
    Station::new("TUC", "Tung Chung", "東涌"),
    Station::new("TUM", "Tuen Mun", "屯門"),
    Station::new("TWH", "Tai Wo Hau", "大窩口"),
    Station::new("TWO", "Tai Wo", "太和"),
    Station::new("TWW", "Tsuen Wan West", "荃灣西"),
    Station::new("UNI", "University", "大學"),
    Station::new("WAC", "Wan Chai", "灣仔"),
    Station::new("WCH", "Wong Chuk Hang", "黃竹坑"),
    Station::new("WHA", "Whampoa", "黃埔"),
    Station::new("WKS", "Wu Kai Sha", "烏溪沙"),
    Station::new("WTS", "Wong Tai Sin", "黃大仙"),
    Station::new("YAT", "Yau Tong", "油塘"),
    Station::new("YMT", "Yau Ma Tei", "油麻地"),
    Station::new("YUL", "Yuen Long", "元朗"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codes_case_insensitively() {
        let code: StationCode = "tko".parse().expect("code should parse");

        assert_eq!(code.as_str(), "TKO");
        assert_eq!(code, StationCode::from_static("TKO"));
    }

    #[test]
    fn rejects_malformed_codes() {
        for input in ["", "TK", "TKOO", "T1O", "將軍澳"] {
            assert_eq!(input.parse::<StationCode>(), Err(InvalidStationCode));
        }
    }

    #[test]
    fn finds_known_stations_only() {
        let known = Station::find(StationCode::from_static("PRE")).expect("PRE is known");
        assert_eq!(known.name, Localized::new("Prince Edward", "太子"));

        assert!(Station::find(StationCode::from_static("XYZ")).is_none());
    }

    #[test]
    fn station_table_is_sorted_and_unique() {
        assert!(STATIONS.windows(2).all(|pair| pair[0].code < pair[1].code));
    }
}
