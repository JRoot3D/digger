//! The manual's option table against the command line: every argument
//! clap knows is listed, in the form `--help` shows it, with the upstream
//! alias its help text names, and nothing else is listed.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

use clap::{Arg, CommandFactory};

use super::Cli;
use crate::manual_doc::{manual, table};

/// The argument as the usage line of `--help` writes it: `--speed <N>`,
/// `--gauntlet[=<SECS>]`, `-h, --help`, `[SPEED|LEVELS]`.
fn usage_form(arg: &Arg) -> String {
    let value = arg
        .get_value_names()
        .and_then(|names| names.first())
        .map(ToString::to_string);
    let Some(long) = arg.get_long() else {
        let value = value.unwrap_or_else(|| arg.get_id().to_string());
        return format!("[{value}]");
    };
    let mut form = match arg.get_short() {
        Some(short) => format!("-{short}, --{long}"),
        None => format!("--{long}"),
    };
    if arg.get_action().takes_values()
        && let Some(value) = value
    {
        let optional = arg.get_num_args().is_some_and(|n| n.min_values() == 0);
        if optional && arg.is_require_equals_set() {
            form = format!("{form}[=<{value}>]");
        } else {
            form = format!("{form} <{value}>");
        }
    }
    form
}

/// The upstream alias named at the end of the argument's help text
/// (`[alias: /S:N]`, `[aliases: /?, /H]`), or `—` for none.
fn alias(arg: &Arg) -> String {
    let help = arg.get_help().map(ToString::to_string).unwrap_or_default();
    let help = help.trim_end();
    ["[alias: ", "[aliases: "]
        .iter()
        .find_map(|marker| {
            let (_, rest) = help.rsplit_once(marker)?;
            Some(rest.strip_suffix(']').unwrap_or(rest).to_owned())
        })
        .unwrap_or_else(|| "—".to_owned())
}

#[test]
fn manual_option_list_matches_cli() {
    let command = Cli::command();
    let expected: Vec<(String, String)> = command
        .get_arguments()
        .map(|arg| (usage_form(arg), alias(arg)))
        .collect();
    let documented: Vec<(String, String)> = table("options")
        .into_iter()
        .map(|row| {
            assert_eq!(row.len(), 3, "option row {row:?}");
            assert!(!row[2].is_empty(), "{} has no meaning", row[0]);
            (row[0].clone(), row[1].clone())
        })
        .collect();
    assert_eq!(documented, expected);
    // The spot checks of the forms, so a change in clap's accessors cannot
    // make both sides agree on nonsense.
    assert!(expected.contains(&("--speed <N>".to_owned(), "/S:N".to_owned())));
    assert!(expected.contains(&("--gauntlet[=<SECS>]".to_owned(), "/G[:SECS]".to_owned())));
    assert!(expected.contains(&("-h, --help".to_owned(), "/?, /H".to_owned())));
    assert!(expected.contains(&("[SPEED|LEVELS]".to_owned(), "—".to_owned())));

    // The upstream letters that are not options of their own.
    let text = manual();
    for letter in ["`/V`", "`/C`", "`/N`", "`/X`"] {
        assert!(
            text.contains(letter),
            "the manual does not mention {letter}"
        );
    }
}
