// Copyright 2025 Oxide Computer Company

use std::{fmt::Display, path::PathBuf};

use predicates::{reflection::PredicateReflection, Predicate};

/// Creates a new predicate that ensures equality with the given file.
///
/// To accept changes to the file, run with `EXPECTORATE=overwrite`.
#[cfg_attr(docsrs, doc(cfg(feature = "predicates")))]
pub fn eq_file(path: impl Into<PathBuf>) -> FilePredicate {
    let path = path.into();
    FilePredicate { path, panic: false }
}

/// Creates a new predicate that ensures equality with the given file and
/// panics if there's a mismatch.
///
/// To accept changes to the file, run with `EXPECTORATE=overwrite`.
#[cfg_attr(docsrs, doc(cfg(feature = "predicates")))]
pub fn eq_file_or_panic(path: impl Into<PathBuf>) -> FilePredicate {
    let path = path.into();
    FilePredicate { path, panic: true }
}

pub struct FilePredicate {
    path: PathBuf,
    panic: bool,
}

impl Display for FilePredicate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.path.display(), self.panic)
    }
}

impl Predicate<str> for FilePredicate {
    fn eval(&self, actual: &str) -> bool {
        match crate::assert_contents_impl(
            &self.path,
            actual,
            crate::OverwriteMode::from_env(),
        ) {
            Err(e) if self.panic => {
                panic!("assertion failed: {e}")
            }
            Err(e) => {
                println!("{e}");
                false
            }
            Ok(_) => true,
        }
    }

    fn find_case<'a>(
        &'a self,
        expected: bool,
        variable: &str,
    ) -> Option<predicates::reflection::Case<'a>> {
        let actual = self.eval(variable);
        if expected == actual {
            Some(predicates::reflection::Case::new(None, actual))
        } else {
            None
        }
    }
}

impl PredicateReflection for FilePredicate {}

#[cfg(test)]
mod test {
    use predicates::Predicate;

    use crate::eq_file;

    #[test]
    fn predicates_good() {
        let actual = include_str!("../tests/data_a.txt");
        assert!(eq_file("tests/data_a.txt").eval(actual));
    }

    #[test]
    #[should_panic]
    fn predicates_bad() {
        let actual = include_str!("../tests/data_a.txt");
        assert!(eq_file("tests/data_b.txt").eval(actual));
    }

    #[test]
    #[should_panic]
    fn predicates_one_line_change() {
        let actual = include_str!("../tests/data_a.txt");
        assert!(eq_file("tests/data_a2.txt").eval(actual));
    }
}
