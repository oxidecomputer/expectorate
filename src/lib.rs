// Copyright 2023 Oxide Computer Company

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

use console::Style;
use newline_converter::dos2unix;
use similar::{Algorithm, ChangeTag, TextDiff};
use std::{env, ffi::OsStr, fs, path::Path};

/// Compare the contents of the file to the string provided
#[track_caller]
pub fn assert_contents<P: AsRef<Path>>(path: P, actual: &str) {
    if let Err(e) = assert_contents_impl(path, actual) {
        panic!("assertion failed: {e}")
    }
}

pub(crate) fn assert_contents_impl<P: AsRef<Path>>(path: P, actual: &str) -> Result<(), String> {
    let path = path.as_ref();
    let var = env::var_os("EXPECTORATE");
    let overwrite = var.as_deref().and_then(OsStr::to_str) == Some("overwrite");

    let actual = dos2unix(actual);

    if overwrite {
        if let Err(e) = fs::write(path, actual.as_ref()) {
            panic!("unable to write to {}: {}", path.display(), e);
        }
    } else {
        // Treat a nonexistent file like an empty file.
        let expected_s = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => String::new(),
                _ => panic!("unable to read contents of {}: {}", path.display(), e),
            },
        };
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
    Ok(())
}
