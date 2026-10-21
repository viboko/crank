// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A template's crank header: settings for crank itself, written as YAML
//! inside a Tera comment at the very start of the template —
//!
//! ```text
//! {#- crank:
//!   pocketMarkers:
//!     - start: '# {name} ['
//!       end: '# ]'
//! -#}
//! ```
//!
//! Tera skips a comment, so the header never reaches the output, and the
//! template stays a plain Tera template. Only a comment that opens the
//! file (after a byte order mark, if any) and whose text starts with
//! `crank:` is a header; any other comment is just a comment.

use std::fmt;

use serde::Deserialize;

use crate::pocket::{MarkerError, MarkerPattern, PocketMarkers};

/// What a template's header says, once parsed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Header {
    /// `pocketMarkers:` — the marker pairs that replace crank's built-in
    /// ones for this template's output, if it lists any.
    pub pocket_markers: Option<Vec<MarkerPattern>>,
}

impl Header {
    /// The pocket markers this template's output is written with:
    /// `pocketMarkers`, compiled, or `None` for crank's built-in ones.
    ///
    /// # Errors
    /// Returns [`MarkerError`] if a listed pair can't be used.
    pub fn compiled_markers(&self) -> Result<Option<PocketMarkers>, MarkerError> {
        self.pocket_markers
            .as_deref()
            .map(PocketMarkers::compile)
            .transpose()
    }
}

/// The YAML inside a header comment: a single `crank:` key.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHeader {
    /// The settings under `crank:`.
    crank: RawSettings,
}

/// The settings under a header's `crank:` key.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawSettings {
    /// See [`Header::pocket_markers`].
    #[serde(default)]
    pocket_markers: Option<Vec<MarkerPattern>>,
}

/// A header that couldn't be read: a comment that's never closed, or YAML
/// that isn't the expected shape.
#[derive(Debug)]
pub enum HeaderError {
    /// The header's comment has no closing `#}`.
    Unclosed,
    /// The header's YAML isn't valid, or isn't the expected shape.
    Yaml(serde_yaml_ng::Error),
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderError::Unclosed => {
                write!(f, "the header comment is never closed with `#}}`")
            }
            HeaderError::Yaml(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for HeaderError {}

/// The YAML text of `source`'s header comment, if it opens with one: the
/// comment's text between `{#`/`{#-` and `#}`/`-#}`, less any spaces
/// before its first line's text. Line breaks are kept, so a line number
/// in a YAML error is the template's own line number too.
///
/// # Errors
/// Returns [`HeaderError::Unclosed`] if the header comment never closes.
fn header_text(source: &str) -> Result<Option<&str>, HeaderError> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let Some(comment) = source.strip_prefix("{#") else {
        return Ok(None);
    };
    let comment = comment.strip_prefix('-').unwrap_or(comment);
    if !comment.trim_start().starts_with("crank:") {
        return Ok(None);
    }
    let text = comment.trim_start_matches([' ', '\t']);
    let Some(close) = text.find("#}") else {
        return Err(HeaderError::Unclosed);
    };
    let inside = text.get(..close).expect("`find` returns a char boundary");
    Ok(Some(inside.strip_suffix('-').unwrap_or(inside)))
}

/// Reads the crank header at the start of `source` (a template's text). A
/// template without one gets the default [`Header`].
///
/// # Errors
/// Returns [`HeaderError`] if `source` opens with a header that never
/// closes, or whose YAML isn't a `crank:` mapping of known settings.
pub fn parse(source: &str) -> Result<Header, HeaderError> {
    let Some(text) = header_text(source)? else {
        return Ok(Header::default());
    };
    let raw: RawHeader = serde_yaml_ng::from_str(text).map_err(HeaderError::Yaml)?;
    Ok(Header {
        pocket_markers: raw.crank.pocket_markers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH_HEADER: &str =
        "{#- crank:\n  pocketMarkers:\n    - start: '# {name} ['\n      end: '# ]'\n-#}\n";

    #[test]
    fn a_template_without_a_header_has_the_default_one() {
        assert_eq!(parse("Hello {{ name }}\n").unwrap(), Header::default());
        assert_eq!(parse("").unwrap(), Header::default());
    }

    #[test]
    fn a_header_lists_pocket_markers() {
        let header = parse(&format!("{HASH_HEADER}# notes [\n# ]\n")).unwrap();
        assert_eq!(
            header.pocket_markers,
            Some(vec![MarkerPattern::new("# {name} [", "# ]")])
        );
    }

    #[test]
    fn a_header_without_whitespace_control_dashes_works_too() {
        let header =
            parse("{# crank: {pocketMarkers: [{start: '-- {name} [', end: '-- ]'}]} #}").unwrap();
        assert_eq!(
            header.pocket_markers,
            Some(vec![MarkerPattern::new("-- {name} [", "-- ]")])
        );
    }

    #[test]
    fn crank_can_start_on_the_line_after_the_comment_opens() {
        let header = parse("{#-\ncrank:\n  pocketMarkers: []\n-#}\n").unwrap();
        assert_eq!(header.pocket_markers, Some(vec![]));
    }

    #[test]
    fn a_header_after_a_byte_order_mark_still_counts() {
        let header = parse(&format!("\u{feff}{HASH_HEADER}")).unwrap();
        assert!(header.pocket_markers.is_some());
    }

    #[test]
    fn a_header_that_doesnt_open_the_file_is_just_a_comment() {
        assert_eq!(
            parse(&format!("\n{HASH_HEADER}")).unwrap(),
            Header::default()
        );
        assert_eq!(
            parse(&format!("text {HASH_HEADER}")).unwrap(),
            Header::default()
        );
    }

    #[test]
    fn a_leading_comment_not_starting_with_crank_is_just_a_comment() {
        assert_eq!(
            parse("{# crankshaft: yes #}\n{# a note #}\n").unwrap(),
            Header::default()
        );
    }

    #[test]
    fn crank_with_no_pocket_markers_keeps_the_built_in_ones() {
        assert_eq!(parse("{# crank: {} #}").unwrap(), Header::default());
    }

    #[test]
    fn an_unclosed_header_is_an_error() {
        assert!(matches!(
            parse("{#- crank:\n  pocketMarkers: []\n"),
            Err(HeaderError::Unclosed)
        ));
    }

    #[test]
    fn an_unknown_setting_is_an_error() {
        let err = parse("{# crank: {pockets: []} #}").unwrap_err();
        assert!(matches!(err, HeaderError::Yaml(_)));
        assert!(err.to_string().contains("pockets"), "{err}");
    }

    #[test]
    fn a_second_top_level_key_is_an_error() {
        assert!(parse("{# crank: {}\nother: 1 #}").is_err());
    }

    #[test]
    fn a_yaml_error_gives_the_templates_own_line_number() {
        let err = parse("{#-\ncrank:\n  pocketMarkers: 3\n-#}\n").unwrap_err();
        assert!(err.to_string().contains("line 3"), "{err}");
    }

    #[test]
    fn invalid_yaml_is_an_error() {
        assert!(matches!(
            parse("{# crank: [unclosed #}"),
            Err(HeaderError::Yaml(_))
        ));
    }

    #[test]
    fn compiled_markers_are_none_without_pocket_markers() {
        assert!(Header::default().compiled_markers().unwrap().is_none());
    }

    #[test]
    fn compiled_markers_reports_a_bad_pair() {
        let header = Header {
            pocket_markers: Some(vec![MarkerPattern::new("# [", "# ]")]),
        };
        assert!(matches!(
            header.compiled_markers(),
            Err(MarkerError::StartNeedsOneName(_))
        ));
    }
}
