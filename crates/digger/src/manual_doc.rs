//! Tests that keep the user manual, `docs/manual.md`, in step with the
//! program: its settings table must list the schema's settings in file
//! order with their ranges and defaults (design D9), and its option table
//! is checked against the command line in `cli::manual_tests`.
//!
//! A checked table sits between `<!-- table:<name> -->` and
//! `<!-- /table:<name> -->`. Cells are split on unescaped `|`; `\|` stands
//! for a literal bar and backticks are dropped.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::PathBuf;

/// The manual's text.
pub(crate) fn manual() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/manual.md");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The body rows of the manual's table `name`, each as its cells.
pub(crate) fn table(name: &str) -> Vec<Vec<String>> {
    let text = manual();
    let start = format!("<!-- table:{name} -->");
    let end = format!("<!-- /table:{name} -->");
    let Some((_, rest)) = text.split_once(&start) else {
        panic!("docs/manual.md has no {start}");
    };
    let Some((body, _)) = rest.split_once(&end) else {
        panic!("docs/manual.md has no {end}");
    };
    body.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('|'))
        // The header row and the separator row.
        .skip(2)
        .map(cells)
        .collect()
}

/// The cells of one table row.
fn cells(line: &str) -> Vec<String> {
    let inner = line.trim_start_matches('|');
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '|' => cells.push(std::mem::take(&mut cell)),
            '`' => {}
            _ => cell.push(c),
        }
    }
    cells.push(cell);
    cells.into_iter().map(|c| c.trim().to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::{cells, table};
    use crate::config::schema::schema;

    #[test]
    fn cells_unescape_bars_and_drop_backticks() {
        assert_eq!(
            cells("| `a` \\| `b` | `[\"X\"]` | text |"),
            ["a | b", "[\"X\"]", "text"]
        );
    }

    /// Every setting of the schema appears in the manual's settings table,
    /// in file order, with its range text and its default as TOML.
    #[test]
    fn manual_settings_table_matches_schema() {
        let rows = table("settings");
        let documented: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|row| {
                assert_eq!(row.len(), 4, "settings row {row:?}");
                assert!(!row[3].is_empty(), "{} has no meaning", row[0]);
                (row[0].as_str(), row[1].as_str(), row[2].as_str())
            })
            .collect();
        let expected: Vec<(String, String, String)> = schema()
            .iter()
            .map(|entry| {
                (
                    entry.path(),
                    entry.range_text(),
                    entry.default_value().to_string(),
                )
            })
            .collect();
        let expected: Vec<(&str, &str, &str)> = expected
            .iter()
            .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
            .collect();
        assert_eq!(documented, expected);
    }
}
