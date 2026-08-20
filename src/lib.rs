// Copyright 2026 Oxide Computer Company

#![cfg_attr(docsrs, feature(doc_cfg))]

//! This library is for comparing multi-line output to data stored in version
//! controlled files. It makes it easy to update the contents when should be
//! updated to match the new results.
//!
//! Use it like this:
//!
//! ```rust
//! # fn compose() -> &'static str { "" }
//! let actual: &str = compose();
//! expectorate::assert_contents("lyrics.txt", actual);
//! ```
//!
//! If the output doesn't match, the program will panic! and emit the
//! color-coded diffs.
//!
//! To accept the changes from `compose()`, run with `EXPECTORATE=overwrite`.
//! Assuming `lyrics.txt` is checked in, `git diff` will show you something
//! like this:
//!
//! ```diff
//! diff --git a/examples/lyrics.txt b/examples/lyrics.txt
//! index e4104c1..ea6beaf 100644
//! --- a/examples/lyrics.txt
//! +++ b/examples/lyrics.txt
//! @@ -1,5 +1,2 @@
//! -No one hits like Gaston
//! -Matches wits like Gaston
//! -In a spitting match nobody spits like Gaston
//! +In a testing match nobody tests like Gaston
//! I'm especially good at expectorating
//! -Ten points for Gaston
//! ```
//!
//! # `predicates` feature
//!
//! Enable the `predicates` feature for compatibility with `predicates` via
//! [`eq_file`] and [`eq_file_or_panic`].
//! # Predicates (feature: predicates)

//! Expectorate can be used in places where you might use the [`predicates`
//! crate](https://crates.io/crates/predicates). If you're using
//! `predicates::path::eq_file` you can instead use `expectorate::eq_file` or
//! `expectorate::eq_file_or_panic`. Populate or update the specified file as
//! above.

#[cfg(feature = "predicates")]
mod feature_predicates;
#[cfg(feature = "predicates")]
pub use feature_predicates::*;

use atomicwrites::{AtomicFile, OverwriteBehavior};
use console::Style;
use newline_converter::dos2unix;
use similar::{Algorithm, ChangeTag, TextDiff};
use std::{env, ffi::OsStr, fs, io::Write, path::Path, time::Duration};

/// Maximum time to spend computing a diff.
///
/// `similar` caps its own Myers search at `isqrt(n * m)` steps, so a large
/// edit script no longer makes the search quadratic in the input length. What
/// remains is a mildly superlinear tail: shapes that defeat the cap -- a file
/// whose line order is reversed, or one with insertions and deletions
/// alternating throughout -- reach roughly a second at a few hundred thousand
/// lines. `similar` honors this as a best-effort deadline, falling back to a
/// coarser (but still valid) approximation once it expires, so this bounds
/// that tail rather than preventing a hang.
const DIFF_TIMEOUT: Duration = Duration::from_millis(500);

/// Maximum number of diff lines to print.
///
/// No one reads past the first few hundred lines; beyond that, `git diff` on
/// an overwritten file is a better tool than a wall of test output.
const MAX_DIFF_LINES: usize = 500;

/// Bounds on the work spent producing a diff, and on how much of it is shown.
///
/// Only the defaults are used in production; tests construct other values so
/// that they can exercise these bounds without relying on an input large
/// enough to reach them naturally.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DiffLimits {
    /// Best-effort deadline for computing the diff.
    timeout: Duration,
    /// Upper bound on the number of lines printed.
    max_lines: usize,
}

impl Default for DiffLimits {
    fn default() -> Self {
        Self {
            timeout: DIFF_TIMEOUT,
            max_lines: MAX_DIFF_LINES,
        }
    }
}

/// Compare the contents of the file to the string provided
#[track_caller]
pub fn assert_contents<P: AsRef<Path>>(path: P, actual: &str) {
    if let Err(e) = assert_contents_impl(
        path,
        actual,
        OverwriteMode::from_env(),
        DiffLimits::default(),
    ) {
        panic!("assertion failed: {e}")
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum OverwriteMode {
    Check,
    Overwrite,
}

impl OverwriteMode {
    pub(crate) fn from_env() -> Self {
        let var = env::var_os("EXPECTORATE");
        if var.as_deref().and_then(OsStr::to_str) == Some("overwrite") {
            OverwriteMode::Overwrite
        } else {
            OverwriteMode::Check
        }
    }
}

pub(crate) fn assert_contents_impl<P: AsRef<Path>>(
    path: P,
    actual: &str,
    mode: OverwriteMode,
    limits: DiffLimits,
) -> Result<(), String> {
    let path = path.as_ref();
    let actual = dos2unix(actual);

    let current = match fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => None,
            _ => panic!("unable to read contents of {}: {}", path.display(), e),
        },
    };

    match mode {
        OverwriteMode::Overwrite => {
            // Don't write the file if it's the same contents. This avoids mtime
            // invalidation.
            if current.as_deref() != Some(&actual) {
                // There's no way to do a compare-and-set kind of operation on
                // filesystems where you can say "only overwrite this file if the
                // inode matches what was just read". The closest approximation is
                // to disallow overwrites if the file doesn't exist.
                let behavior = if current.is_some() {
                    OverwriteBehavior::AllowOverwrite
                } else {
                    OverwriteBehavior::DisallowOverwrite
                };
                let f = AtomicFile::new(path, behavior);
                let res = f.write(|f| {
                    // We're writing the contents out in one call, so there's no
                    // need to have a BufWriter wrapper.
                    f.write(actual.as_bytes())
                });
                if let Err(e) = res {
                    panic!("unable to write to {}: {}", path.display(), e);
                }
            }
        }
        OverwriteMode::Check => {
            // Treat a nonexistent file like an empty file.
            let expected_s = current.unwrap_or_default();
            let expected = dos2unix(&expected_s);

            if expected != actual {
                let mut printed = 0;
                let mut truncated = false;
                for hunk in TextDiff::configure()
                    .algorithm(Algorithm::Myers)
                    .timeout(limits.timeout)
                    .diff_lines(&expected, &actual)
                    .unified_diff()
                    .context_radius(5)
                    .iter_hunks()
                {
                    println!("{}", hunk.header());
                    printed += 1 + hunk.iter_changes().count();
                    if printed >= limits.max_lines {
                        println!("<remaining output too large>");
                        truncated = true;
                        break;
                    }

                    for change in hunk.iter_changes() {
                        let (marker, style) = match change.tag() {
                            ChangeTag::Delete => ('-', Style::new().red()),
                            ChangeTag::Insert => ('+', Style::new().green()),
                            ChangeTag::Equal => (' ', Style::new()),
                        };
                        print!("{}", style.apply_to(marker).bold());
                        print!("{}", style.apply_to(change));
                        if change.missing_newline() {
                            println!();
                        }
                    }
                }
                println!();

                let truncation = if truncated {
                    format!(
                        "\n                diff truncated after {} lines; \
                         overwrite and use e.g. `git diff` to see the whole \
                         change",
                        limits.max_lines,
                    )
                } else {
                    String::new()
                };
                return Err(format!(
                    r#"string doesn't match the contents of file: "{}" see diffset above{}
                set EXPECTORATE=overwrite if these changes are intentional"#,
                    path.display(),
                    truncation,
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use filetime::{set_file_mtime, FileTime};
    use std::ops::Range;
    use tempfile::TempDir;

    fn lines(range: Range<usize>, value: u64) -> String {
        range
            .map(|i| format!("line {i} of value {value}\n"))
            .collect()
    }

    /// Generate `n` lines, replacing those for which `changed` returns true.
    /// Every other line matches what `lines(0..n, 1)` produces, so two of
    /// these differ only where `changed` says they do.
    fn lines_changing(n: usize, changed: impl Fn(usize) -> bool) -> String {
        (0..n)
            .map(|i| {
                if changed(i) {
                    format!("changed line {i}\n")
                } else {
                    format!("line {i} of value 1\n")
                }
            })
            .collect()
    }

    /// Compare `actual` against a file holding `expected`, returning the
    /// failure message. Panics if the two match.
    fn check(expected: &str, actual: &str, limits: DiffLimits) -> String {
        let dir = TempDir::with_prefix("expectorate-").unwrap();
        let path = dir.path().join("my-file.txt");
        fs::write(&path, expected).unwrap();
        assert_contents_impl(&path, actual, OverwriteMode::Check, limits)
            .unwrap_err()
    }

    /// If EXPECTORATE=overwrite is set and the file is unchanged, ensure that
    /// the mtime stays the same.
    #[test]
    fn overwite_same_mtime_doesnt_change() {
        static CONTENTS: &str = "foo";
        // Setting the mtime to 1970-01-01 doesn't appear to work on Windows.
        // Instead, set it to 2000-01-01. The exact time doesn't really matter
        // here as much as having a fixed value that's in the past.`
        const MTIME: FileTime = FileTime::from_unix_time(946684800, 0);

        let dir = TempDir::with_prefix("expectorate-").unwrap();
        let path = dir.path().join("my-file.txt");
        fs::write(&path, CONTENTS).unwrap();

        // Set the mtime to a fixed value.
        set_file_mtime(&path, MTIME).unwrap();

        // Overwrite the contents with the same value.
        assert_contents_impl(
            &path,
            CONTENTS,
            OverwriteMode::Overwrite,
            DiffLimits::default(),
        )
        .unwrap();

        let meta = fs::metadata(&path).unwrap();
        let mtime2 = FileTime::from_last_modification_time(&meta);

        assert_eq!(mtime2, MTIME, "mtime is zero");
    }

    /// Two changes separated by thousands of matching lines diff exactly into
    /// two small hunks, but only if the Myers search is allowed to run. When
    /// the deadline expires, similar dumps out the full spans which will exceed
    /// our output limit.
    #[test]
    fn timeout_is_applied_to_the_diff() {
        const N: usize = 10_000;
        let expected = lines_changing(N, |_| false);
        let actual = lines_changing(N, |i| i == 5 || i == N - 5);

        let exact = check(&expected, &actual, DiffLimits::default());
        assert!(
            !exact.contains("diff truncated"),
            "two changes {N} lines apart should produce 2 small hunks: {exact}",
        );

        let timedout = check(
            &expected,
            &actual,
            DiffLimits {
                timeout: Duration::ZERO,
                ..Default::default()
            },
        );
        assert!(
            timedout.contains("diff truncated"),
            "an expired deadline should produce a huge diff: {timedout}",
        );
    }

    #[test]
    fn test_output_length_limited() {
        let expected = lines_changing(2_000, |_| false);
        let actual = lines_changing(2_000, |i| i % 100 == 50);

        let full = check(&expected, &actual, DiffLimits::default());
        assert!(
            !full.contains("diff truncated"),
            "twenty small hunks should fit under the default cap: {full}"
        );

        let capped = check(
            &expected,
            &actual,
            DiffLimits {
                max_lines: 50,
                ..Default::default()
            },
        );
        assert!(
            capped.contains("diff truncated after 50 lines"),
            "expected the cap to be reported: {capped}"
        );
    }

    /// A wholly regenerated file has no line in common with the original, so
    /// it becomes a single enormous hunk that must be truncated rather than
    /// printed in full.
    #[test]
    fn wholly_different_file_is_truncated() {
        let err = check(
            &lines(0..50_000, 1),
            &lines(0..50_000, 2),
            DiffLimits::default(),
        );

        assert!(err.contains("diff truncated"), "unexpected error: {err}");
    }

    /// A localized change to a large file is cheap to diff exactly, so it must
    /// still produce a complete diff: these bounds are for pathological
    /// shapes, not for big files.
    #[test]
    fn large_file_small_change_is_not_truncated() {
        let mut actual = lines(0..50_000, 1);
        actual.push_str("one more line\n");
        actual.push_str(&lines(50_000..100_000, 1));

        let err = check(&lines(0..100_000, 1), &actual, DiffLimits::default());

        assert!(
            !err.contains("diff truncated"),
            "diff should not have been truncated: {err}"
        );
    }
}
