//! Turning what somebody typed into a `LIKE` pattern.
//!
//! One function, in one place, because the alternative is what this replaced:
//! five private copies of the same four lines, any one of which could be
//! forgotten by the sixth listing to grow a search box - and a forgotten one
//! does not fail, it quietly matches every row.

/// `%needle%`, with the wildcards in `needle` neutralised.
///
/// Without the escaping, typing `%` into a search box matches everything and
/// typing `_` matches any single character, which reads as a search box that
/// sometimes ignores what was typed. A search for `50%` is the one people
/// actually run into.
pub fn contains(needle: &str) -> String {
    format!("%{}%", escaped(needle))
}

/// The wildcards in a search term, neutralised.
pub fn escaped(needle: &str) -> String {
    needle
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wildcard_somebody_typed_is_a_character_and_not_a_wildcard() {
        assert_eq!(escaped("50%"), "50\\%");
        assert_eq!(escaped("a_b"), "a\\_b");
        assert_eq!(escaped("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn a_search_matches_anywhere_in_the_value() {
        assert_eq!(contains("smith"), "%smith%");
    }
}
