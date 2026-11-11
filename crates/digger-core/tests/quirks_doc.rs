//! The quirks document cannot drift from the code (design D14; task 15.4).
//!
//! `docs/compat-quirks.md` lists every simulation quirk Q01–Q40 with the
//! place that implements it and the tests that pin it, and the undefined
//! and accidental cases U01–U12 likewise. This test reads the document and
//! the crate's sources and checks that every test the tables name exists,
//! that no quirk row has an empty "Implemented in" or "Test" cell, and that
//! every `quirk_q*` test in the crate is listed in its quirk's row.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The quirks document.
const DOC: &str = include_str!("../../../docs/compat-quirks.md");

/// Where a test function is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TestFn {
    /// The module path inside the library (`sim::penalty::tests`), or
    /// `None` for an integration test under `tests/`.
    module: Option<String>,
    /// The file, relative to the crate.
    file: String,
}

/// Every Rust file under `dir`, recursively.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable source directory") {
        let path = entry.expect("readable entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The module path of library file `relative` (`src/sim/penalty.rs` is
/// `sim::penalty`, `src/video/mod.rs` is `video`, `src/lib.rs` is empty).
fn module_of(relative: &Path) -> String {
    let mut parts: Vec<String> = relative
        .iter()
        .skip(1)
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    let last = parts.pop().expect("a file name");
    let stem = last.trim_end_matches(".rs");
    if stem != "mod" && stem != "lib" {
        parts.push(stem.to_owned());
    }
    parts.join("::")
}

/// Every `#[test]` function of the crate, by name.
fn tests_of_crate() -> BTreeMap<String, Vec<TestFn>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    rust_files(&root.join("tests"), &mut files);
    let mut tests: BTreeMap<String, Vec<TestFn>> = BTreeMap::new();
    for path in files {
        let relative = path.strip_prefix(root).expect("inside the crate");
        let in_library = relative.starts_with("src");
        let source = fs::read_to_string(&path).expect("readable source");
        let mut inline_tests = false;
        let mut pending_test = false;
        for line in source.lines() {
            let line = line.trim();
            if line.starts_with("mod tests {") {
                inline_tests = true;
            }
            if line == "#[test]" {
                pending_test = true;
                continue;
            }
            if pending_test {
                if line.starts_with("#[") {
                    continue;
                }
                pending_test = false;
                let name = line
                    .strip_prefix("fn ")
                    .and_then(|rest| rest.split('(').next())
                    .unwrap_or_else(|| panic!("{}: #[test] before {line:?}", relative.display()));
                let module = in_library.then(|| {
                    let mut module = module_of(relative);
                    if inline_tests {
                        if !module.is_empty() {
                            module.push_str("::");
                        }
                        module.push_str("tests");
                    }
                    module
                });
                tests.entry(name.to_owned()).or_default().push(TestFn {
                    module,
                    file: relative.display().to_string(),
                });
            }
        }
    }
    tests
}

/// The rows of the Markdown table whose first cell is an id starting with
/// `prefix` followed by two digits: the id and the cells after it.
fn rows(prefix: char) -> Vec<(String, Vec<String>)> {
    DOC.lines()
        .filter_map(|line| {
            let inner = line.strip_prefix("| ")?.strip_suffix(" |")?;
            let cells: Vec<String> = inner.split(" | ").map(|c| c.trim().to_owned()).collect();
            let id = cells.first()?;
            let digits = id.strip_prefix(prefix)?;
            (digits.len() == 2 && digits.bytes().all(|b| b.is_ascii_digit()))
                .then(|| (id.clone(), cells.get(1..).unwrap_or_default().to_vec()))
        })
        .collect()
}

/// The text between backticks in `cell`.
fn code_spans(cell: &str) -> Vec<&str> {
    cell.split('`').skip(1).step_by(2).collect()
}

/// The test names in a "Test" cell: every code span that is not a file.
fn named_tests(cell: &str) -> Vec<&str> {
    code_spans(cell)
        .into_iter()
        .filter(|span| !span.contains('/') && Path::new(span).extension().is_none())
        .collect()
}

/// Whether `reference` (`name` or `module::path::tests::name`) names a test.
fn exists(tests: &BTreeMap<String, Vec<TestFn>>, reference: &str) -> bool {
    let (module, name) = match reference.rsplit_once("::") {
        Some((module, name)) => (Some(module), name),
        None => (None, reference),
    };
    let Some(found) = tests.get(name) else {
        return false;
    };
    match module {
        None => !found.is_empty(),
        Some(module) => found.iter().any(|t| t.module.as_deref() == Some(module)),
    }
}

/// Checks the rows of one table: `columns` cells after the id, the
/// "Implemented in" and "Test" cells at `implemented` and `test`, and every
/// named test existing. With `strict`, both cells must name something.
fn check_table(
    prefix: char,
    count: usize,
    columns: usize,
    implemented: usize,
    test: usize,
    strict: bool,
) -> Vec<(String, Vec<String>)> {
    let tests = tests_of_crate();
    let rows = rows(prefix);
    let ids: Vec<&str> = rows.iter().map(|(id, _)| id.as_str()).collect();
    let expected: Vec<String> = (1..=count).map(|n| format!("{prefix}{n:02}")).collect();
    assert_eq!(ids, expected, "the {prefix} rows, once each and in order");
    let mut problems = Vec::new();
    for (id, cells) in &rows {
        assert_eq!(
            cells.len(),
            columns,
            "{id}: {} cells after the id",
            cells.len()
        );
        let implemented_cell = cells.get(implemented).expect("the Implemented in cell");
        let test_cell = cells.get(test).expect("the Test cell");
        if implemented_cell.is_empty() {
            problems.push(format!("{id}: empty Implemented in cell"));
        }
        if test_cell.is_empty() {
            problems.push(format!("{id}: empty Test cell"));
        }
        let named = named_tests(test_cell);
        if strict && named.is_empty() {
            problems.push(format!("{id}: the Test cell names no test"));
        }
        if strict && code_spans(implemented_cell).is_empty() {
            problems.push(format!("{id}: the Implemented in cell names no code"));
        }
        for name in named {
            if !exists(&tests, name) {
                problems.push(format!("{id}: no test {name}"));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    rows
}

#[test]
fn quirks_doc_lists_every_quirk_test() {
    // Id, Behaviour, Research reference, Implemented in, Test.
    let rows = check_table('Q', 40, 4, 2, 3, true);

    // Every quirk_q<id> test of the crate is listed in its quirk's row.
    let listed: BTreeMap<&str, BTreeSet<&str>> = rows
        .iter()
        .map(|(id, cells)| {
            let names = cells
                .get(3)
                .map(|cell| named_tests(cell))
                .unwrap_or_default()
                .into_iter()
                .map(|name| name.rsplit("::").next().unwrap_or(name))
                .collect();
            (id.as_str(), names)
        })
        .collect();
    let mut missing = Vec::new();
    for (name, places) in tests_of_crate() {
        let Some(rest) = name.strip_prefix("quirk_q") else {
            continue;
        };
        let id = format!("Q{}", rest.get(..2).unwrap_or(rest));
        let in_row = listed
            .get(id.as_str())
            .is_some_and(|names| names.contains(name.as_str()));
        if !in_row {
            missing.push(format!(
                "{name} ({}) is not listed under {id}",
                places.first().map_or("?", |place| place.file.as_str())
            ));
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn quirks_doc_names_existing_tests_for_undefined_behaviour() {
    // Id, Behaviour, Research reference, Decision, Class, Implemented in,
    // Test. A dropped feature has no code or test in this change.
    check_table('U', 12, 6, 4, 5, false);
}
