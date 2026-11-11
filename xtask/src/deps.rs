//! The dependency-direction rule: the simulation and sound libraries depend on
//! no platform crate (design.md, "Dependency-direction check").

/// Packages whose normal-dependency tree must stay free of platform crates.
pub(crate) const CHECKED_PACKAGES: &[&str] = &["digger-core", "digger-sound"];

/// Platform crates that `digger-core` and `digger-sound` must never depend
/// on, directly or transitively. This is the single place the list lives.
pub(crate) const PLATFORM_DENY_LIST: &[&str] =
    &["winit", "softbuffer", "cpal", "gilrs", "directories"];

/// Returns the deny-listed crate names found in the output of
/// `cargo tree --edges normal --prefix none`, sorted and without duplicates.
///
/// Each non-empty line of that output starts with a package name followed by
/// a space and its version (`winit v0.30.0`), optionally followed by a path
/// and a `(*)` marker for repeated subtrees.
pub(crate) fn denied_crates(tree_output: &str) -> Vec<String> {
    let mut found: Vec<String> = tree_output
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| PLATFORM_DENY_LIST.contains(name))
        .map(str::to_owned)
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

#[cfg(test)]
mod tests {
    use super::denied_crates;

    #[test]
    fn clean_tree_has_no_denied_crates() {
        let tree = "\
digger-core v0.1.0 (/work/digger/crates/digger-core)
";
        assert_eq!(denied_crates(tree), Vec::<String>::new());
    }

    #[test]
    fn clean_tree_with_ordinary_dependencies_passes() {
        let tree = "\
digger-sound v0.1.0 (/work/digger/crates/digger-sound)
thiserror v2.0.12
thiserror-impl v2.0.12 (proc-macro)
proc-macro2 v1.0.95
unicode-ident v1.0.18
quote v1.0.40
proc-macro2 v1.0.95 (*)
";
        assert_eq!(denied_crates(tree), Vec::<String>::new());
    }

    #[test]
    fn violating_tree_reports_each_platform_crate_once() {
        let tree = "\
digger-core v0.1.0 (/work/digger/crates/digger-core)
winit v0.30.11
bitflags v2.9.1
cpal v0.16.0
dasp_sample v0.11.0
winit v0.30.11 (*)
gilrs v0.11.0
";
        assert_eq!(denied_crates(tree), ["cpal", "gilrs", "winit"]);
    }

    #[test]
    fn similarly_named_crates_are_not_flagged() {
        let tree = "\
digger-core v0.1.0 (/work/digger/crates/digger-core)
winit-utils v0.1.0
cpal-sys v0.1.0
directories-next v2.0.0
";
        assert_eq!(denied_crates(tree), Vec::<String>::new());
    }

    #[test]
    fn transitive_directories_dependency_is_flagged() {
        let tree = "\
digger-sound v0.1.0 (/work/digger/crates/digger-sound)
some-config v1.0.0
directories v6.0.0
";
        assert_eq!(denied_crates(tree), ["directories"]);
    }
}
