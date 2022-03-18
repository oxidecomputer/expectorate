// Copyright 2021 Oxide Computer Company

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
//! To accept the changes from `compose()`, run with ! `EXPECTORATE=overwrite`.
//! Assuming `lyrics.txt` is checked in, `git diff` will show you something like
//! this:
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

use newline_converter::dos2unix;
use similar::{udiff::unified_diff, Algorithm};
use std::{env, fs, path::Path};

/// Compare the contents of the file to the string provided
#[track_caller]
pub fn assert_contents<P: AsRef<Path>>(path: P, actual: &str) {
    let path = path.as_ref();
    let var = env::var_os("EXPECTORATE");
    let overwrite = match var.as_ref().map(|s| s.as_os_str().to_str()) {
        Some(Some("overwrite")) => true,
        _ => false,
    };

    let actual = dos2unix(actual);

    if overwrite {
        if let Err(e) = fs::write(path, actual.as_ref()) {
            panic!("unable to write to {}: {}", path.display(), e.to_string());
        }
    } else {
        // Treat non-existant files like an empty file.
        let expected_s = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => String::new(),
                _ => panic!(
                    "unable to read contents of {}: {}",
                    path.display(),
                    e.to_string()
                ),
            },
        };
        let expected = dos2unix(&expected_s);

        if expected != actual {
            let diff =
                unified_diff(Algorithm::Myers, &expected, &actual, 5, None);
            println!("{}", diff);
            panic!(
                r#"assertion failed: string doesn't match the contents of file: "{}" see diffset above
                set EXPECTORATE=overwrite if these changes are intentional"#,
                path.display()
            );
        }
    }
}
