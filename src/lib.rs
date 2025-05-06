// Copyright 2023-2025 Oxide Computer Company

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
use std::{env, ffi::OsStr, fs, io::Write, path::Path};

/// Compare the contents of the file to the string provided
#[track_caller]
pub fn assert_contents<P: AsRef<Path>>(path: P, actual: &str) {
    if let Err(e) =
        assert_contents_impl(path, actual, OverwriteMode::from_env())
    {
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
                for hunk in TextDiff::configure()
                    .algorithm(Algorithm::Myers)
                    .diff_lines(&expected, &actual)
                    .unified_diff()
                    .context_radius(5)
                    .iter_hunks()
                {
                    println!("{}", hunk.header());
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
                return Err(format!(
                    r#"string doesn't match the contents of file: "{}" see diffset above
                set EXPECTORATE=overwrite if these changes are intentional"#,
                    path.display()
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
    use tempfile::TempDir;

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
        assert_contents_impl(&path, CONTENTS, OverwriteMode::Overwrite)
            .unwrap();

        let meta = fs::metadata(&path).unwrap();
        let mtime2 = FileTime::from_last_modification_time(&meta);

        assert_eq!(mtime2, MTIME, "mtime is zero");
    }
}
