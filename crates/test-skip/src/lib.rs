//! One macro, with one purpose: make the right shape of a skip shorter than the
//! wrong one.
//!
//! The project rule is that skipping is fine and skipping in silence is not. A
//! test that cannot run announces `SKIPPED (not a pass)` on stderr, because a
//! silent skip is indistinguishable from a passing test -- which is how the
//! `/ToUnicode` duplication survived a fully green suite.
//!
//! Twice on 25-08-2026 it went wrong anyway, in two files, and not out of
//! carelessness but out of shape. The announcing form was three lines:
//!
//! ```ignore
//! let Some(x) = load("y.pdf") else {
//!     eprintln!("SKIPPED (not a pass): y.pdf is not readable");
//!     return;
//! };
//! ```
//!
//! and the silent one was one word:
//!
//! ```ignore
//! let Some(x) = load("y.pdf") else { return };
//! ```
//!
//! While that holds, the silent form wins whenever somebody is in a hurry. Now
//! it is the other way round:
//!
//! ```ignore
//! let Some(x) = load("y.pdf") else { skip_test!("y.pdf is not readable") };
//! ```

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// Announce a skip and return from the test.
///
/// Usable where an expression is expected, so it fits the `else` arm of a
/// `let ... else`, where a bare `return` is not allowed but a block is.
///
/// The wording is exactly `SKIPPED (not a pass): <reason>` on purpose:
/// `scripts/ci/test_skip_lint.py` looks for that, and a variant that sits just
/// beside it is not recognised and is therefore a silent skip again.
#[macro_export]
macro_rules! skip_test {
    ($($arg:tt)*) => {{
        eprintln!("SKIPPED (not a pass): {}", format_args!($($arg)*));
        return;
    }};
}

#[cfg(test)]
mod tests {
    /// It fits inside a `let ... else`, which is the whole reason it exists.
    ///
    /// A test function returns `()`, so the bare `return` is right there. These
    /// helpers do the same: that is the shape the macro is meant for.
    #[test]
    fn it_fits_in_a_let_else() {
        fn attempt(value: Option<u32>) {
            let Some(v) = value else {
                crate::skip_test!("no value")
            };
            assert_eq!(v, 3);
        }
        attempt(Some(3));
        // With `None` it returns without reaching the assert. That cannot be
        // asserted separately without ending the test -- and that is exactly
        // the behaviour we want.
        attempt(None);
    }

    /// It takes a format string, so the reason can be concrete.
    #[test]
    fn it_takes_a_format_string() {
        fn attempt(path: &str, present: bool) {
            if !present {
                crate::skip_test!("{path} is not readable");
            }
            assert!(present);
        }
        attempt("y.pdf", true);
        attempt("gone.pdf", false);
    }
}
