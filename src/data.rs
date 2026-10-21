// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parsing of a task group's data file, and its conversion into a Tera
//! rendering context.

use std::fmt;
use std::path::Path;

use serde_yaml_ng::Value;

/// The template variable crank reserves for itself: data can't define a
/// top-level key by this name, and [`with_crank`] fills it in with what
/// crank knows about the file being rendered.
pub const RESERVED_NAME: &str = "crank";

/// A data file that failed to parse as YAML.
#[derive(Debug)]
pub struct ParseError(serde_yaml_ng::Error);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseError {}

/// Parses `source` (the raw contents of a task group's data file) into a
/// [`Value`], preserving whatever shape (mapping, sequence, scalar) it has.
///
/// # Errors
/// Returns [`ParseError`] if `source` isn't valid YAML.
pub fn parse(source: &str) -> Result<Value, ParseError> {
    serde_yaml_ng::from_str(source).map_err(ParseError)
}

/// A parsed data [`Value`] that couldn't be turned into a Tera rendering
/// context — this happens when the data file's top level isn't a mapping
/// (e.g. a bare list or scalar), since template variables are looked up by
/// name, or when it defines the reserved [`RESERVED_NAME`] key.
#[derive(Debug)]
pub struct ContextError(tera::Error);

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ContextError {}

/// Converts a parsed data [`Value`] into the [`tera::Context`] a template is
/// rendered against, exposing each top-level key as a template variable.
///
/// # Errors
/// Returns [`ContextError`] if `value`'s top level isn't a mapping, or has
/// a key named [`RESERVED_NAME`].
pub fn to_context(value: &Value) -> Result<tera::Context, ContextError> {
    let context = tera::Context::from_serialize(value).map_err(ContextError)?;
    if context.contains_key(RESERVED_NAME) {
        return Err(ContextError(tera::Error::message(format!(
            "`{RESERVED_NAME}` is a reserved name — rename that top-level key"
        ))));
    }
    Ok(context)
}

/// `context` with the reserved [`RESERVED_NAME`] variable added, describing
/// the file being rendered to `output_path`: `crank.filename` is its last
/// component (`widget.rs` for `src/widget.rs`), or empty if it has none.
#[must_use]
pub fn with_crank(context: &tera::Context, output_path: &Path) -> tera::Context {
    let filename = output_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut context = context.clone();
    context.insert(
        RESERVED_NAME,
        &std::collections::BTreeMap::from([("filename", filename)]),
    );
    context
}

/// A parsed data [`Value`] that couldn't be used as an `array` task
/// group's list of items — this happens when the data file's top level
/// isn't a sequence (e.g. a mapping or scalar), since an `array` field
/// with no `#selector` uses the whole document as its list of items.
#[derive(Debug)]
pub struct ItemsError {
    /// What the top level was instead, e.g. `"a mapping"`.
    found: &'static str,
}

impl fmt::Display for ItemsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "resolves to {}, not an array — crank needs a sequence of items to run an `array` task group",
            self.found
        )
    }
}

impl std::error::Error for ItemsError {}

/// Converts a parsed data [`Value`] into the list of items an `array` task
/// group with no `#selector` runs its tasks over — the whole document,
/// which must already be a sequence.
///
/// # Errors
/// Returns [`ItemsError`] if `value`'s top level isn't a sequence.
pub fn to_items(value: &Value) -> Result<&[Value], ItemsError> {
    value.as_sequence().map_or_else(
        || {
            Err(ItemsError {
                found: crate::selector::kind(value),
            })
        },
        |sequence| Ok(sequence.as_slice()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_maps_lists_and_scalars() {
        let source = "
name: widget
count: 3
tags:
  - alpha
  - beta
nested:
  enabled: true
";
        let value = parse(source).unwrap();
        assert_eq!(value["name"].as_str(), Some("widget"));
        assert_eq!(value["count"].as_i64(), Some(3));
        assert_eq!(value["tags"][0].as_str(), Some("alpha"));
        assert_eq!(value["nested"]["enabled"].as_bool(), Some(true));
    }

    #[test]
    fn invalid_yaml_is_a_parse_error() {
        let source = "name: [this is not: valid: yaml:";
        assert!(parse(source).is_err());
    }

    #[test]
    fn a_top_level_mapping_converts_to_a_context() {
        let value = parse("name: widget\ncount: 3").unwrap();
        let context = to_context(&value).unwrap();
        assert_eq!(context.get("name").and_then(|v| v.as_str()), Some("widget"));
    }

    #[test]
    fn a_top_level_scalar_fails_to_convert_to_a_context() {
        let value = parse("just a string").unwrap();
        assert!(to_context(&value).is_err());
    }

    #[test]
    fn a_top_level_sequence_converts_to_items() {
        let value = parse("- name: Hawaiian\n- name: Diabolo\n").unwrap();
        let items = to_items(&value).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["name"].as_str(), Some("Hawaiian"));
    }

    #[test]
    fn a_top_level_mapping_fails_to_convert_to_items() {
        let value = parse("name: widget").unwrap();
        assert!(to_items(&value).is_err());
    }

    #[test]
    fn a_top_level_scalar_fails_to_convert_to_items() {
        let value = parse("just a string").unwrap();
        assert!(to_items(&value).is_err());
    }

    #[test]
    fn non_ascii_keys_and_values_parse() {
        let value =
            parse("名前: ピザ\nцена: 12.5\n메뉴:\n  - 김치\n  - 🍕\nżółw: \"Zażółć\"\nemoji: 👩‍👩‍👧‍👦\n")
                .unwrap();
        assert_eq!(value["名前"].as_str(), Some("ピザ"));
        assert_eq!(value["цена"].as_f64(), Some(12.5));
        assert_eq!(value["메뉴"][1].as_str(), Some("🍕"));
        assert_eq!(value["żółw"].as_str(), Some("Zażółć"));
        assert_eq!(value["emoji"].as_str(), Some("👩‍👩‍👧‍👦"));
        let context = to_context(&value).unwrap();
        assert_eq!(
            context.get("名前").and_then(tera::Value::as_str),
            Some("ピザ")
        );
    }

    #[test]
    fn yaml_unicode_escapes_decode() {
        let value = parse("a: \"caf\\u00e9\"\nb: \"\\U0001F355\"\n").unwrap();
        assert_eq!(value["a"].as_str(), Some("café"));
        assert_eq!(value["b"].as_str(), Some("🍕"));
    }

    #[test]
    fn a_leading_byte_order_mark_is_ignored() {
        let value = parse("\u{feff}名前: ピザ\n").unwrap();
        assert_eq!(value["名前"].as_str(), Some("ピザ"));
    }

    #[test]
    fn a_parse_error_shows_the_yaml_parser_message() {
        let expected = serde_yaml_ng::from_str::<Value>("a: [")
            .unwrap_err()
            .to_string();
        assert_eq!(parse("a: [").unwrap_err().to_string(), expected);
    }

    #[test]
    fn a_context_error_shows_teras_message() {
        let value = parse("- a\n- b\n").unwrap();
        let err = to_context(&value).unwrap_err();
        assert_eq!(err.to_string(), err.0.to_string());
        assert!(err.to_string().contains("got array"), "{err}");
    }

    #[test]
    fn a_top_level_crank_key_is_reserved() {
        let value = parse("crank: mine\nname: widget\n").unwrap();
        let err = to_context(&value).unwrap_err();
        assert!(
            err.to_string().contains("`crank` is a reserved name"),
            "{err}"
        );
    }

    #[test]
    fn a_nested_crank_key_is_not_reserved() {
        let value = parse("widget:\n  crank: mine\n").unwrap();
        assert!(to_context(&value).is_ok());
    }

    #[test]
    fn with_crank_sets_the_output_file_name() {
        let context = to_context(&parse("name: widget").unwrap()).unwrap();
        let context = with_crank(&context, Path::new("src/gen/widget.rs"));
        assert_eq!(
            context
                .get("crank")
                .and_then(|crank| crank.get_from_path("filename"))
                .and_then(tera::Value::as_str),
            Some("widget.rs")
        );
        assert_eq!(context.get("name"), Some(&tera::Value::from("widget")));
    }

    #[test]
    fn with_crank_leaves_the_file_name_empty_for_a_path_without_one() {
        let context = with_crank(&tera::Context::new(), Path::new(""));
        assert_eq!(
            context
                .get("crank")
                .and_then(|crank| crank.get_from_path("filename"))
                .and_then(tera::Value::as_str),
            Some("")
        );
    }
}
