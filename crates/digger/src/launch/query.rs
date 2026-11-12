//! The browser's start-up options: the page address's query string
//! (`web-build` spec, start-up options from URL query parameters;
//! add-web-build-and-releases, design decision 9). Pure, so it is compiled
//! and tested on every target.

#![cfg_attr(
    not(any(test, target_arch = "wasm32")),
    expect(dead_code, reason = "only the browser reads a query string")
)]

use super::LaunchWarning;
use crate::config::{GameMode, Layer, Speed};

/// The lowest start level.
const LEVEL_MIN: u16 = 1;
/// The highest start level.
const LEVEL_MAX: u16 = 1000;

/// The start-up options of the page address's query string (`web-build`
/// spec, start-up options from URL query parameters): `speed` and `level`
/// as the desktop's `--speed` and `--start-level`, `mode` by the
/// configuration file's mode names, and `quiet` (present, or `1`: start
/// with sound off; `0`: no effect). Names and values are percent-decoded
/// (`+` is a space). A value outside the allowed set is ignored with one
/// [`LaunchWarning`]; unknown parameters are ignored silently; a parameter
/// given twice takes its last valid value. Nothing here can fail or panic
/// on what the address holds.
///
/// The `Layer` is laid over the stored settings in memory and never saved.
pub(crate) fn parse_query(query: &str) -> (Layer, Vec<LaunchWarning>) {
    let query = query.strip_prefix('?').unwrap_or(query);
    let mut layer = Layer::default();
    let mut warnings = Vec::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(name);
        let value = percent_decode(value);
        let warn = |reason: &str| LaunchWarning {
            name: name.clone(),
            value: value.clone(),
            reason: reason.to_owned(),
        };
        match name.as_str() {
            "speed" => match number(&value).and_then(Speed::new) {
                Some(speed) => layer.speed = Some(speed),
                None => warnings.push(warn("a number from 0 to 500 is expected")),
            },
            "level" => match number(&value).filter(|level| (LEVEL_MIN..=LEVEL_MAX).contains(level))
            {
                Some(level) => layer.start_level = Some(level),
                None => warnings.push(warn("a number from 1 to 1000 is expected")),
            },
            "mode" => match GameMode::from_name(&value) {
                Some(mode) => layer.mode = Some(mode),
                None => warnings.push(warn(&format!(
                    "one of {} is expected",
                    GameMode::NAMES.join(", ")
                ))),
            },
            "quiet" => match value.as_str() {
                "" | "1" => layer.sound_enabled = Some(false),
                "0" => layer.sound_enabled = None,
                _ => warnings.push(warn("no value, 1 or 0 is expected")),
            },
            _ => {}
        }
    }
    (layer, warnings)
}

/// A whole number written with decimal digits only (no sign, no spaces),
/// if it fits.
fn number(text: &str) -> Option<u16> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// `text` with `+` as a space and every `%XX` escape decoded; an escape
/// that is not two hexadecimal digits stays as written, and bytes that are
/// not UTF-8 become replacement characters.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        let escaped = match (byte, bytes.get(i + 1), bytes.get(i + 2)) {
            (b'%', Some(&high), Some(&low)) => hex(high).zip(hex(low)),
            _ => None,
        };
        match (byte, escaped) {
            (_, Some((high, low))) => {
                out.push(high << 4 | low);
                i += 3;
            }
            (b'+', None) => {
                out.push(b' ');
                i += 1;
            }
            (byte, None) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The value of one hexadecimal digit.
const fn hex(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_query, percent_decode};
    use crate::config::{GameMode, Layer, Settings, Speed};
    use crate::launch::LaunchWarning;

    #[test]
    fn query_gauntlet_speed() {
        let (layer, warnings) = parse_query("?mode=gauntlet&speed=30");
        assert_eq!(warnings, []);
        assert_eq!(layer.mode, Some(GameMode::Gauntlet));
        assert_eq!(layer.speed, Speed::new(30));
        assert_eq!(
            layer.speed.map(|speed| speed.frame_time().as_micros()),
            Some(60_000)
        );
        // Nothing else is set, so a reload without parameters applies the
        // stored mode and speed again.
        let only = Layer {
            mode: layer.mode,
            speed: layer.speed,
            ..Layer::default()
        };
        assert_eq!(layer, only);
        let settings = Settings::resolve(&Layer::default(), &layer);
        assert_eq!(settings.mode, GameMode::Gauntlet);
        assert_eq!(settings.gauntlet_time, 120);
        assert_eq!(parse_query("").0, Layer::default());
        assert_eq!(parse_query("?").0, Layer::default());
        // Every desktop mode name, and the speed limits.
        for name in GameMode::NAMES {
            let (layer, warnings) = parse_query(&format!("mode={name}"));
            assert_eq!(layer.mode.map(GameMode::name), Some(name));
            assert_eq!(warnings, []);
        }
        assert_eq!(parse_query("speed=0").0.speed, Speed::new(0));
        assert_eq!(parse_query("speed=500").0.speed, Speed::new(500));
        assert_eq!(parse_query("speed=501").1.len(), 1);
    }

    #[test]
    fn query_level_and_quiet() {
        let (layer, warnings) = parse_query("?level=5&quiet");
        assert_eq!(warnings, []);
        assert_eq!(layer.start_level, Some(5));
        assert_eq!(layer.sound_enabled, Some(false));
        let settings = Settings::resolve(&Layer::default(), &layer);
        assert_eq!(settings.start_level, 5);
        assert!(!settings.sound_enabled);
        for (bad, decoded) in [
            ("0", "0"),
            ("1001", "1001"),
            ("-1", "-1"),
            ("%2B5", "+5"),
            ("five", "five"),
            ("", ""),
            ("5+", "5 "),
        ] {
            let (layer, warnings) = parse_query(&format!("level={bad}"));
            assert_eq!(layer.start_level, None, "{bad}");
            assert_eq!(
                warnings,
                [LaunchWarning {
                    name: String::from("level"),
                    value: decoded.to_owned(),
                    reason: String::from("a number from 1 to 1000 is expected"),
                }]
            );
        }
        assert_eq!(parse_query("level=1000").0.start_level, Some(1000));
    }

    #[test]
    fn query_quiet_flag_and_value() {
        for query in ["quiet", "?quiet", "quiet=", "quiet=1", "speed=40&quiet"] {
            let (layer, warnings) = parse_query(query);
            assert_eq!(layer.sound_enabled, Some(false), "{query}");
            assert_eq!(warnings, [], "{query}");
        }
        let (layer, warnings) = parse_query("quiet=0");
        assert_eq!(layer.sound_enabled, None);
        assert_eq!(warnings, []);
        let (layer, warnings) = parse_query("quiet=yes");
        assert_eq!(layer.sound_enabled, None);
        assert_eq!(
            warnings.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["quiet=yes ignored: no value, 1 or 0 is expected"]
        );
    }

    #[test]
    fn query_bad_speed_warns_and_keeps_mode() {
        let (layer, warnings) = parse_query("?speed=fast&mode=two-players&colour=red");
        assert_eq!(layer.speed, None);
        assert_eq!(layer.mode, Some(GameMode::TwoPlayers));
        assert_eq!(
            warnings,
            [LaunchWarning {
                name: String::from("speed"),
                value: String::from("fast"),
                reason: String::from("a number from 0 to 500 is expected"),
            }]
        );
        // The stored speed applies.
        let stored = Layer {
            speed: Speed::new(25),
            ..Layer::default()
        };
        assert_eq!(
            Some(Settings::resolve(&stored, &layer).speed),
            Speed::new(25)
        );
    }

    #[test]
    fn query_unknown_mode_warns() {
        let (layer, warnings) = parse_query("mode=three-players");
        assert_eq!(layer.mode, None);
        assert_eq!(
            warnings.iter().map(ToString::to_string).collect::<Vec<_>>(),
            [
                "mode=three-players ignored: one of one-player, two-players, two-diggers, \
                 gauntlet, two-digger-gauntlet is expected"
            ]
        );
        // Names are matched exactly, after decoding.
        assert_eq!(parse_query("mode=Gauntlet").1.len(), 1);
        assert_eq!(
            parse_query("mode=two%2Ddiggers").0.mode,
            Some(GameMode::TwoDiggers)
        );
    }

    #[test]
    fn query_unknown_key_ignored() {
        let (layer, warnings) = parse_query("colour=red&utm_source=x&&=&%ZZ=1&Speed=fast");
        assert_eq!(layer, Layer::default());
        assert_eq!(warnings, []);
        // A value given twice: the last valid one counts, each bad one
        // warns.
        let (layer, warnings) = parse_query("speed=20&speed=x&speed=30");
        assert_eq!(layer.speed, Speed::new(30));
        assert_eq!(warnings.len(), 1);
        // Percent-decoding of names and values; `+` is a space.
        assert_eq!(percent_decode("a%20b+c%2b"), "a b c+");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%e2%82%ac"), "\u{20ac}");
        assert_eq!(percent_decode("%ff"), "\u{fffd}");
        let (_, warnings) = parse_query("sp%65ed=f%C3%A4st");
        assert_eq!(warnings.first().map(|w| w.value.as_str()), Some("fäst"));
    }
}
