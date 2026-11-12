//! The format a save name selects and upstream's default file name for a
//! saved recording (game-recording spec, "Saving and default file names";
//! research 05 §4.5).
//!
//! Only the names are decided here; writing the file is the caller's
//! (the crate does no I/O).

use super::header::RecordingKind;
use crate::sim::GameMode;

/// The name a game of two alternating players is saved under when no name
/// is given.
pub const DEFAULT_TWO_PLAYER_NAME: &str = "DIGGER.DRF";
/// The suffix that selects the eDRF format, in any letter case.
const EDRF_SUFFIX: &str = ".edrf";
/// The suffix of a default DRF name.
const DRF_SUFFIX: &str = ".drf";
/// From this score on only two initials go into a default name, which
/// keeps it within eight characters before the suffix.
const TWO_INITIALS_FROM: i64 = 100_000;
/// The character that stands for a missing initial or one that is not a
/// letter.
const NO_INITIAL: char = '_';

/// The format a recording saved under `name` is written in: eDRF when the
/// name ends in `.edrf` (in any letter case) after at least one other
/// character, DRF otherwise.
pub fn kind_for_name(name: &str) -> RecordingKind {
    let bytes = name.as_bytes();
    let suffix = EDRF_SUFFIX.len();
    let is_edrf = bytes.len() > suffix
        && bytes
            .get(bytes.len() - suffix..)
            .is_some_and(|end| end.eq_ignore_ascii_case(EDRF_SUFFIX.as_bytes()));
    if is_edrf {
        RecordingKind::Edrf
    } else {
        RecordingKind::Drf
    }
}

/// The name a game is saved under when none is given: `DIGGER.DRF` for a
/// game of two alternating players, otherwise the initials and the final
/// score followed by `.drf`.
///
/// Each of the three initials that is missing or not an ASCII letter
/// becomes `_`. Below 100000 points all three are used; from 100000 on
/// two: the first two if the third is `_`, else the last two if the first
/// is `_`, else the first and the third.
pub fn default_file_name(mode: GameMode, initials: Option<&str>, score: i32) -> String {
    if mode.alternating() {
        return DEFAULT_TWO_PLAYER_NAME.to_owned();
    }
    let mut letters = [NO_INITIAL; 3];
    if let Some(initials) = initials {
        for (letter, c) in letters.iter_mut().zip(initials.chars()) {
            if c.is_ascii_alphabetic() {
                *letter = c;
            }
        }
    }
    let [first, second, third] = letters;
    let mut name = String::new();
    if i64::from(score) < TWO_INITIALS_FROM {
        name.extend([first, second, third]);
    } else if third == NO_INITIAL {
        name.extend([first, second]);
    } else if first == NO_INITIAL {
        name.extend([second, third]);
    } else {
        name.extend([first, third]);
    }
    name.push_str(&score.to_string());
    name.push_str(DRF_SUFFIX);
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GauntletSeconds;

    #[test]
    fn default_names() {
        let one = GameMode::OnePlayer;
        assert_eq!(default_file_name(one, Some("mmm"), 27700), "mmm27700.drf");
        assert_eq!(
            default_file_name(one, Some("abc"), 1_859_075),
            "ac1859075.drf"
        );
        assert_eq!(default_file_name(one, Some("ab1"), 250_000), "ab250000.drf");
        assert_eq!(default_file_name(one, Some("1bc"), 250_000), "bc250000.drf");
        assert_eq!(default_file_name(one, None, 3100), "___3100.drf");
        assert_eq!(default_file_name(one, Some(""), 3100), "___3100.drf");
        assert_eq!(default_file_name(one, Some("k"), 99_999), "k__99999.drf");
        assert_eq!(default_file_name(one, Some("k"), 100_000), "k_100000.drf");
        assert_eq!(
            default_file_name(GameMode::TwoPlayers, Some("abc"), 5000),
            "DIGGER.DRF"
        );
        // Two diggers and the gauntlets are named like one-player games.
        assert_eq!(
            default_file_name(GameMode::TwoDiggers, Some("x.y"), 12),
            "x_y12.drf"
        );
        let gauntlet = GameMode::Gauntlet {
            time: GauntletSeconds::new(120),
        };
        assert_eq!(default_file_name(gauntlet, Some("ABC"), 0), "ABC0.drf");
    }

    #[test]
    fn edrf_suffix_selects_the_format() {
        for name in ["game.edrf", "x.EDRF", "dir/a.eDrf"] {
            assert_eq!(kind_for_name(name), RecordingKind::Edrf, "{name}");
        }
        for name in [
            ".edrf",
            "game.drf",
            "game.edrf.drf",
            "edrf",
            "DIGGER.DRF",
            "",
        ] {
            assert_eq!(kind_for_name(name), RecordingKind::Drf, "{name}");
        }
    }
}
