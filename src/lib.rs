// Copyright 2025 Oxide Computer Company

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

/// Trait for types that can be used as content for comparison.
pub trait AsContent {
    /// Get the content as bytes for writing to file.
    fn as_bytes(&self) -> &[u8];

    /// Check if this content should be treated as text (true) or binary (false).
    fn is_text(&self) -> bool;
}

impl AsContent for &str {
    fn as_bytes(&self) -> &[u8] {
        (*self).as_bytes()
    }

    fn is_text(&self) -> bool {
        true
    }
}

impl AsContent for &[u8] {
    fn as_bytes(&self) -> &[u8] {
        self
    }

    fn is_text(&self) -> bool {
        false
    }
}

impl AsContent for &Vec<u8> {
    fn as_bytes(&self) -> &[u8] {
        self.as_slice()
    }

    fn is_text(&self) -> bool {
        false
    }
}

/// Compare the contents of the file to the string provided
#[track_caller]
pub fn assert_contents<P: AsRef<Path>, C: AsContent>(path: P, actual: C) {
    if let Err(e) =
        assert_contents_impl(path, &actual, OverwriteMode::from_env())
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
    actual: &dyn AsContent,
    mode: OverwriteMode,
) -> Result<(), String> {
    let path = path.as_ref();
    let actual_bytes = actual.as_bytes();

    let current = match fs::read(path) {
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
            if current.as_deref() != Some(actual_bytes) {
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
                    f.write_all(actual_bytes)
                });
                if let Err(e) = res {
                    panic!("unable to write to {}: {}", path.display(), e);
                }
            }
        }
        OverwriteMode::Check => {
            // Treat a nonexistent file like an empty file.
            let expected = current.unwrap_or_default();

            if expected != actual_bytes {
                if actual.is_text() {
                    // Handle text comparison with line-by-line diff, fallback to binary on error
                    if let Err(text_err) =
                        show_text_diff(&expected, actual_bytes)
                    {
                        eprintln!("Text diff failed: {}", text_err);
                        eprintln!("Falling back to binary diff:");
                        show_binary_diff(&expected, actual_bytes)?;
                    }
                } else {
                    // Handle binary comparison with binary diff
                    show_binary_diff(&expected, actual_bytes)?;
                }

                return Err(format!(
                    r#"content doesn't match the contents of file: "{}" see diff above
                set EXPECTORATE=overwrite if these changes are intentional"#,
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn show_text_diff(expected: &[u8], actual: &[u8]) -> Result<(), String> {
    // Convert both to strings for text comparison
    let expected_str = String::from_utf8_lossy(expected);
    let actual_str = String::from_utf8_lossy(actual);

    // Apply DOS to Unix conversion for consistent line endings
    let expected_normalized = dos2unix(&expected_str);
    let actual_normalized = dos2unix(&actual_str);

    for hunk in TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .diff_lines(&expected_normalized, &actual_normalized)
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
    Ok(())
}

fn show_binary_diff(expected: &[u8], actual: &[u8]) -> Result<(), String> {
    println!("Binary content differs:");

    // Show file sizes - only show both if they differ
    if expected.len() == actual.len() {
        println!("  File size: {} bytes", expected.len());
    } else {
        println!("  Expected size: {} bytes", expected.len());
        println!("  Actual size: {} bytes", actual.len());
    }

    // Find the first difference
    let min_len = expected.len().min(actual.len());
    let mut first_diff_offset = min_len; // Start with the assumption that differences are at the end

    for i in 0..min_len {
        if expected[i] != actual[i] {
            first_diff_offset = i;
            break;
        }
    }

    // Only show diff details if there are actual differences
    if expected != actual {
        let offset = first_diff_offset;
        println!("  First difference at byte offset: {}", offset);

        // For small files, show full content with aligned markers
        if expected.len() <= 32 && actual.len() <= 32 {
            println!("  Expected: {:02x?}", expected);
            println!("  Actual:   {:02x?}", actual);

            // Create difference markers aligned with the hex output
            print!("  Diff:      ");

            let max_len = expected.len().max(actual.len());
            for i in 0..max_len {
                let expected_byte = if i < expected.len() {
                    Some(expected[i])
                } else {
                    None
                };
                let actual_byte = if i < actual.len() {
                    Some(actual[i])
                } else {
                    None
                };

                match (expected_byte, actual_byte) {
                    (Some(e), Some(a)) if e != a => print!("^^"),
                    (None, Some(_)) => print!("^^"), // Extra byte in actual
                    (Some(_), None) => print!("^^"), // Missing byte in actual
                    _ => print!("  "),               // Same or both None
                }

                // Add spacing to match the hex format (no commas on diff line)
                if i < max_len - 1 {
                    print!("  "); // Two spaces to align with ", " in hex output
                }
            }
            println!();
        } else {
            // For larger files, show context around the difference
            let context_start = offset.saturating_sub(8);
            let context_end_expected = (offset + 16).min(expected.len());
            let context_end_actual = (offset + 16).min(actual.len());

            if offset < expected.len() {
                let expected_context =
                    &expected[context_start..context_end_expected];
                println!(
                    "  Expected around offset {}: {:02x?}",
                    context_start, expected_context
                );
            } else {
                println!("  Expected: <end of file>");
            }

            if offset < actual.len() {
                let actual_context = &actual[context_start..context_end_actual];
                println!(
                    "  Actual around offset {}:   {:02x?}",
                    context_start, actual_context
                );
            } else {
                println!("  Actual: <end of file>");
            }

            // Highlight the specific differences in the context window
            if offset < min_len {
                let end_in_context = context_end_expected
                    .min(context_end_actual)
                    - context_start;

                print!("  Difference:                ");

                // Print markers for each byte position
                for i in 0..end_in_context {
                    let abs_offset = context_start + i;
                    if abs_offset < min_len
                        && abs_offset < expected.len()
                        && abs_offset < actual.len()
                    {
                        if expected[abs_offset] != actual[abs_offset] {
                            print!("^^");
                        } else {
                            print!("  ");
                        }

                        // Add separator spacing (", " between elements, except for last)
                        if i < end_in_context - 1 {
                            print!(", ");
                        }
                    } else {
                        // Handle case where one file is shorter
                        print!("^^");
                        if i < end_in_context - 1 {
                            print!(", ");
                        }
                    }
                }
                println!();
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
        assert_contents_impl(&path, &CONTENTS, OverwriteMode::Overwrite)
            .unwrap();

        let meta = fs::metadata(&path).unwrap();
        let mtime2 = FileTime::from_last_modification_time(&meta);

        assert_eq!(mtime2, MTIME, "mtime is zero");
    }
}
