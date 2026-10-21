// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `#`-selector syntax for a task group's `object` field: picking a
//! sub-object out of a YAML data file (`path#pizzas[0]`), and the combined
//! `object:` field value (a path plus an optional selector).
//!
//! A selector is a sequence of steps, separated by `/`: a bare identifier
//! is a [`Step::Key`] (a mapping lookup), and `[N]` is a [`Step::Index`] (a
//! sequence lookup). A key may be followed directly by one or more `[N]`
//! suffixes with no `/` needed (`pizzas[0]`), and a selector may open with
//! a bare `[N]` (no leading key) for a data file whose top level is itself
//! a sequence. Whatever a selector resolves to must be a mapping — crank
//! needs a single object to hand to a template, so resolving to an array
//! or a scalar is an error, not a future capability to leave room for.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer};
use serde_yaml_ng::Value;

/// One step in a [`Selector`]: a mapping key, or a sequence index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// `name`: the value under a mapping key.
    Key(String),
    /// `[n]`: a sequence's item at a zero-based index.
    Index(usize),
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Key(name) => write!(f, "{name}"),
            Step::Index(index) => write!(f, "[{index}]"),
        }
    }
}

/// A parsed `#`-selector: an ordered list of [`Step`]s, plus the original
/// text (used verbatim in error messages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    /// The steps, in order.
    steps: Vec<Step>,
    /// The selector as written.
    text: String,
}

impl Selector {
    /// Parses `text` (the part of a `data`/`object` field after the first
    /// `#`) into a [`Selector`].
    ///
    /// # Errors
    /// Returns [`SelectorSyntaxError`] if `text` isn't a valid
    /// `key`/`[index]` path.
    pub fn parse(text: &str) -> Result<Self, SelectorSyntaxError> {
        let steps = tokenize(text).map_err(|reason| SelectorSyntaxError {
            text: text.to_string(),
            reason,
        })?;
        Ok(Self {
            steps,
            text: text.to_string(),
        })
    }

    /// The steps, in the order they're applied.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

/// Why a selector's text failed to parse.
#[derive(Debug)]
enum SyntaxErrorReason {
    /// There's nothing after the `#`.
    Empty,
    /// A `/` with no step before or after it.
    EmptySegment,
    /// A `[` with no `]` after it.
    UnmatchedBracket,
    /// A `]` with no `[` before it.
    StrayClosingBracket,
    /// `[]`.
    EmptyIndex,
    /// `[...]` holding something other than a number.
    NonNumericIndex(String),
    /// A character that can't start or continue a step there.
    UnexpectedCharacter(char),
}

/// A selector that failed to parse: its text isn't a valid `key`/`[index]`
/// path. Surfaces as a `.crank` parse error, since it's a syntactic
/// problem with the `data`/`object` field's text, decided before any data
/// file is read.
#[derive(Debug)]
pub struct SelectorSyntaxError {
    /// The selector as written.
    text: String,
    /// What's wrong with it.
    reason: SyntaxErrorReason,
}

impl fmt::Display for SelectorSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match &self.reason {
            SyntaxErrorReason::Empty => "is empty".to_string(),
            SyntaxErrorReason::EmptySegment => {
                "has an empty segment (check for a missing, stray, leading, or trailing `/`)"
                    .to_string()
            }
            SyntaxErrorReason::UnmatchedBracket => "has an unmatched `[`".to_string(),
            SyntaxErrorReason::StrayClosingBracket => "has a `]` with no matching `[`".to_string(),
            SyntaxErrorReason::EmptyIndex => "has an empty index `[]`".to_string(),
            SyntaxErrorReason::NonNumericIndex(digits) => {
                format!("has a non-numeric index `[{digits}]`")
            }
            SyntaxErrorReason::UnexpectedCharacter(c) => {
                format!("has an unexpected character `{c}`")
            }
        };
        write!(f, "selector \"{}\" {reason}", self.text)
    }
}

impl std::error::Error for SelectorSyntaxError {}

/// Tokenizes selector text into a flat list of steps. A single-pass scan
/// with two states: at the start of a segment (position 0, or just after a
/// `/`), only a key, an index, or the end of input may follow; right after
/// a step, only `[` (chaining another index), `/` (starting a new
/// segment), or the end of input may follow.
fn tokenize(text: &str) -> Result<Vec<Step>, SyntaxErrorReason> {
    if text.is_empty() {
        return Err(SyntaxErrorReason::Empty);
    }

    let mut chars = text.chars().peekable();
    let mut steps = Vec::new();
    let mut at_segment_start = true;

    while let Some(c) = chars.next() {
        match c {
            '/' if at_segment_start => return Err(SyntaxErrorReason::EmptySegment),
            '/' => at_segment_start = true,
            '[' => {
                let mut digits = String::new();
                loop {
                    match chars.next() {
                        None => return Err(SyntaxErrorReason::UnmatchedBracket),
                        Some(']') => break,
                        Some(digit) => digits.push(digit),
                    }
                }
                if digits.is_empty() {
                    return Err(SyntaxErrorReason::EmptyIndex);
                }
                let index = digits
                    .parse::<usize>()
                    .map_err(|_| SyntaxErrorReason::NonNumericIndex(digits.clone()))?;
                steps.push(Step::Index(index));
                at_segment_start = false;
            }
            ']' => return Err(SyntaxErrorReason::StrayClosingBracket),
            c if at_segment_start && (c.is_alphabetic() || c == '_') => {
                let mut name = String::from(c);
                while let Some(next) = chars.next_if(|&next| is_key_continuation(next)) {
                    name.push(next);
                }
                steps.push(Step::Key(name));
                at_segment_start = false;
            }
            c => return Err(SyntaxErrorReason::UnexpectedCharacter(c)),
        }
    }

    if at_segment_start {
        return Err(SyntaxErrorReason::EmptySegment);
    }

    Ok(steps)
}

/// Whether `c` can continue a key segment after its first character: a
/// letter or digit in any script, `_`, or a combining mark (so a
/// decomposed "é" — `e` then U+0301 — stays one key).
fn is_key_continuation(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || crate::unicode::is_mark(c)
}

/// A task group's `object` field: a path to a YAML data file, plus an
/// optional selector picking a sub-object out of it (`path#selector`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSource {
    /// The data file, relative to the `.crank` file's directory.
    pub path: PathBuf,
    /// The part after `#`, if any.
    pub selector: Option<Selector>,
}

impl ObjectSource {
    /// Splits `raw` at its first `#` into a path and a selector.
    fn parse(raw: &str) -> Result<Self, SelectorSyntaxError> {
        match raw.split_once('#') {
            None => Ok(Self {
                path: PathBuf::from(raw),
                selector: None,
            }),
            Some((path, selector_text)) => Ok(Self {
                path: PathBuf::from(path),
                selector: Some(Selector::parse(selector_text)?),
            }),
        }
    }

    /// Builds an [`ObjectSource`] with no selector, for tests that only
    /// care about the path.
    #[cfg(test)]
    pub(crate) fn from_path(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            selector: None,
        }
    }
}

impl<'de> Deserialize<'de> for ObjectSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        ObjectSource::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// The shape a selector's final resolved value must have — a mapping for
/// [`select_object`], a sequence for [`select_array`]. Used only to phrase
/// a [`SelectErrorDetail::WrongShape`] message.
#[derive(Debug, Clone, Copy)]
enum ExpectedShape {
    /// A mapping, for `object:`.
    Object,
    /// A sequence, for `array:`.
    Array,
}

impl ExpectedShape {
    /// The shape's name with an indefinite article, e.g. "an object".
    fn with_article(self) -> &'static str {
        match self {
            ExpectedShape::Object => "an object",
            ExpectedShape::Array => "an array",
        }
    }

    /// The shape's bare noun, e.g. "object".
    fn noun(self) -> &'static str {
        match self {
            ExpectedShape::Object => "object",
            ExpectedShape::Array => "array",
        }
    }
}

/// Why a selector failed to resolve against the data.
#[derive(Debug)]
enum SelectErrorDetail {
    /// A mapping has no such key.
    KeyNotFound(String),
    /// A sequence is too short for the index.
    IndexOutOfRange {
        /// The index asked for.
        index: usize,
        /// How many items the sequence has.
        len: usize,
    },
    /// A key step reached something other than a mapping.
    ExpectedMapping {
        /// The step.
        step: Step,
        /// What it reached instead, e.g. `"a sequence"`.
        found: &'static str,
    },
    /// An index step reached something other than a sequence.
    ExpectedSequence {
        /// The step.
        step: Step,
        /// What it reached instead.
        found: &'static str,
    },
    /// The final value isn't the shape the group needs.
    WrongShape {
        /// The shape the group needs.
        expected: ExpectedShape,
        /// What the value is instead.
        found: &'static str,
    },
}

/// A selector that failed to resolve against the actual (valid) data: a
/// missing key, an out-of-range index, a step applied to the wrong shape
/// of value, or a final result that isn't an object.
#[derive(Debug)]
pub struct SelectError {
    /// The selector as written.
    text: String,
    /// What went wrong.
    detail: SelectErrorDetail,
}

impl fmt::Display for SelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "selector \"{}\": ", self.text)?;
        match &self.detail {
            SelectErrorDetail::KeyNotFound(key) => write!(f, "key \"{key}\" not found"),
            SelectErrorDetail::IndexOutOfRange { index, len } => {
                let plural = if *len == 1 { "" } else { "s" };
                write!(
                    f,
                    "index [{index}] out of range (sequence has {len} element{plural})"
                )
            }
            SelectErrorDetail::ExpectedMapping { step, found } => {
                write!(f, "step \"{step}\" expects a mapping, found {found}")
            }
            SelectErrorDetail::ExpectedSequence { step, found } => {
                write!(f, "step \"{step}\" expects a sequence, found {found}")
            }
            SelectErrorDetail::WrongShape { expected, found } => write!(
                f,
                "resolves to {found}, not {} — crank needs a single {} to use as template data",
                expected.with_article(),
                expected.noun()
            ),
        }
    }
}

impl std::error::Error for SelectError {}

/// A short, article-including description of `value`'s shape, for error
/// messages (e.g. "an array", "a string"). Also used by [`crate::data`]'s
/// [`ItemsError`](crate::data::ItemsError) message, since it needs the same
/// description for the same [`Value`] shapes.
pub(crate) fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Sequence(_) => "an array",
        Value::Mapping(_) => "a mapping",
        Value::Tagged(_) => "a tagged value",
    }
}

/// Walks `selector`'s steps against `value`, one at a time, with no
/// requirement on the final value's shape — that's left to the caller
/// ([`select_object`]/[`select_array`]), since a bare `Step::Key`/`[N]`
/// walk is identical either way.
///
/// # Errors
/// Returns [`SelectError`] if a step's key/index isn't present, or a step
/// is applied to the wrong shape of value.
fn walk<'v>(value: &'v Value, selector: &Selector) -> Result<&'v Value, SelectError> {
    let mut current = value;

    for step in selector.steps() {
        current = match step {
            Step::Key(name) => {
                let mapping = current.as_mapping().ok_or_else(|| SelectError {
                    text: selector.text.clone(),
                    detail: SelectErrorDetail::ExpectedMapping {
                        step: step.clone(),
                        found: kind(current),
                    },
                })?;
                mapping.get(name.as_str()).ok_or_else(|| SelectError {
                    text: selector.text.clone(),
                    detail: SelectErrorDetail::KeyNotFound(name.clone()),
                })?
            }
            Step::Index(index) => {
                let sequence = current.as_sequence().ok_or_else(|| SelectError {
                    text: selector.text.clone(),
                    detail: SelectErrorDetail::ExpectedSequence {
                        step: step.clone(),
                        found: kind(current),
                    },
                })?;
                sequence.get(*index).ok_or_else(|| SelectError {
                    text: selector.text.clone(),
                    detail: SelectErrorDetail::IndexOutOfRange {
                        index: *index,
                        len: sequence.len(),
                    },
                })?
            }
        };
    }

    Ok(current)
}

/// Resolves `selector` against `value`, walking one step at a time. The
/// final resolved value must itself be a mapping (an object) — a selector
/// that resolves to an array or a scalar is a [`SelectError`], not left
/// for the caller to discover when building a template context. Used for
/// a task group's `object` field.
///
/// # Errors
/// Returns [`SelectError`] if a step's key/index isn't present, a step is
/// applied to the wrong shape of value, or the final resolved value isn't
/// a mapping.
pub fn select_object<'v>(value: &'v Value, selector: &Selector) -> Result<&'v Value, SelectError> {
    let current = walk(value, selector)?;
    if current.as_mapping().is_some() {
        Ok(current)
    } else {
        Err(SelectError {
            text: selector.text.clone(),
            detail: SelectErrorDetail::WrongShape {
                expected: ExpectedShape::Object,
                found: kind(current),
            },
        })
    }
}

/// Resolves `selector` against `value`, walking one step at a time. The
/// final resolved value must itself be a sequence (an array) — a selector
/// that resolves to a mapping or a scalar is a [`SelectError`]. Used for a
/// task group's `array` field.
///
/// # Errors
/// Returns [`SelectError`] if a step's key/index isn't present, a step is
/// applied to the wrong shape of value, or the final resolved value isn't
/// a sequence.
pub fn select_array<'v>(value: &'v Value, selector: &Selector) -> Result<&'v [Value], SelectError> {
    let current = walk(value, selector)?;
    if let Some(sequence) = current.as_sequence() {
        Ok(sequence)
    } else {
        Err(SelectError {
            text: selector.text.clone(),
            detail: SelectErrorDetail::WrongShape {
                expected: ExpectedShape::Array,
                found: kind(current),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pizzas_document() -> Value {
        serde_yaml_ng::from_str(
            "
pizzas:
  - name: Hawaiian
    price: 17.99
  - name: Diabolo
    price: 19.99
",
        )
        .unwrap()
    }

    // -- ObjectSource / selector parsing --------------------------------

    #[test]
    fn no_hash_parses_as_a_path_with_no_selector() {
        let source = ObjectSource::parse("object.yaml").unwrap();
        assert_eq!(source.path, PathBuf::from("object.yaml"));
        assert!(source.selector.is_none());
    }

    #[test]
    fn parses_a_key_only_selector() {
        let source = ObjectSource::parse("object.yaml#pizzas").unwrap();
        assert_eq!(source.path, PathBuf::from("object.yaml"));
        assert_eq!(
            source.selector.unwrap().steps(),
            &[Step::Key("pizzas".to_string())]
        );
    }

    #[test]
    fn parses_a_leading_bare_index_selector() {
        let selector = Selector::parse("[0]").unwrap();
        assert_eq!(selector.steps(), &[Step::Index(0)]);
    }

    #[test]
    fn parses_a_key_with_a_trailing_index() {
        let selector = Selector::parse("pizzas[0]").unwrap();
        assert_eq!(
            selector.steps(),
            &[Step::Key("pizzas".to_string()), Step::Index(0)]
        );
    }

    #[test]
    fn parses_chained_segments_across_a_slash() {
        let selector = Selector::parse("pizzas[0]/name").unwrap();
        assert_eq!(
            selector.steps(),
            &[
                Step::Key("pizzas".to_string()),
                Step::Index(0),
                Step::Key("name".to_string())
            ]
        );
    }

    #[test]
    fn parses_consecutive_indices() {
        let selector = Selector::parse("matrix[0][1]").unwrap();
        assert_eq!(
            selector.steps(),
            &[
                Step::Key("matrix".to_string()),
                Step::Index(0),
                Step::Index(1)
            ]
        );
    }

    #[test]
    fn empty_selector_is_a_syntax_error() {
        assert!(Selector::parse("").is_err());
    }

    #[test]
    fn unmatched_opening_bracket_is_a_syntax_error() {
        assert!(Selector::parse("pizzas[0").is_err());
    }

    #[test]
    fn stray_closing_bracket_is_a_syntax_error() {
        assert!(Selector::parse("pizzas]").is_err());
    }

    #[test]
    fn empty_index_is_a_syntax_error() {
        assert!(Selector::parse("pizzas[]").is_err());
    }

    #[test]
    fn non_numeric_index_is_a_syntax_error() {
        assert!(Selector::parse("pizzas[abc]").is_err());
    }

    #[test]
    fn negative_index_is_a_syntax_error() {
        assert!(Selector::parse("pizzas[-1]").is_err());
    }

    #[test]
    fn leading_slash_is_a_syntax_error() {
        assert!(Selector::parse("/pizzas").is_err());
    }

    #[test]
    fn trailing_slash_is_a_syntax_error() {
        assert!(Selector::parse("pizzas/").is_err());
    }

    #[test]
    fn doubled_slash_is_a_syntax_error() {
        assert!(Selector::parse("pizzas//name").is_err());
    }

    #[test]
    fn key_immediately_after_a_step_with_no_slash_is_a_syntax_error() {
        assert!(Selector::parse("pizzas[0]name").is_err());
    }

    // -- select_object -----------------------------------------------------

    #[test]
    fn selects_a_key_then_index() {
        let document = pizzas_document();
        let selector = Selector::parse("pizzas[0]").unwrap();
        let selected = select_object(&document, &selector).unwrap();
        assert_eq!(selected["name"].as_str(), Some("Hawaiian"));
        assert_eq!(selected["price"].as_f64(), Some(17.99));
    }

    #[test]
    fn selects_via_a_leading_bare_index() {
        let document: Value = serde_yaml_ng::from_str(
            "
- name: Hawaiian
- name: Diabolo
",
        )
        .unwrap();
        let selector = Selector::parse("[1]").unwrap();
        let selected = select_object(&document, &selector).unwrap();
        assert_eq!(selected["name"].as_str(), Some("Diabolo"));
    }

    #[test]
    fn missing_key_is_a_select_error() {
        let document = pizzas_document();
        let selector = Selector::parse("pizzas[0]/nope").unwrap();
        assert!(select_object(&document, &selector).is_err());
    }

    #[test]
    fn index_out_of_range_is_a_select_error() {
        let document = pizzas_document();
        let selector = Selector::parse("pizzas[5]").unwrap();
        assert!(select_object(&document, &selector).is_err());
    }

    #[test]
    fn key_step_against_a_non_mapping_is_a_select_error() {
        let document = pizzas_document();
        let selector = Selector::parse("pizzas[0]/name/nope").unwrap();
        assert!(select_object(&document, &selector).is_err());
    }

    #[test]
    fn index_step_against_a_non_sequence_is_a_select_error() {
        let document = pizzas_document();
        let selector = Selector::parse("pizzas[0][0]").unwrap();
        assert!(select_object(&document, &selector).is_err());
    }

    #[test]
    fn selecting_a_whole_array_as_an_object_is_a_select_error() {
        let document = pizzas_document();
        let selector = Selector::parse("pizzas").unwrap();
        assert!(select_object(&document, &selector).is_err());
    }

    #[test]
    fn selecting_a_scalar_as_an_object_is_a_select_error() {
        let document = pizzas_document();
        let selector = Selector::parse("pizzas[0]/name").unwrap();
        assert!(select_object(&document, &selector).is_err());
    }

    // -- select_array --------------------------------------------------------

    #[test]
    fn selects_a_nested_array_via_a_key() {
        let document = pizzas_document();
        let selector = Selector::parse("pizzas").unwrap();
        let selected = select_array(&document, &selector).unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0]["name"].as_str(), Some("Hawaiian"));
        assert_eq!(selected[1]["name"].as_str(), Some("Diabolo"));
    }

    #[test]
    fn selects_a_nested_array_of_scalars() {
        let document: Value = serde_yaml_ng::from_str("tags: [alpha, beta]").unwrap();
        let selector = Selector::parse("tags").unwrap();
        let selected = select_array(&document, &selector).unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].as_str(), Some("alpha"));
    }

    #[test]
    fn selecting_a_single_object_as_an_array_is_a_select_error() {
        let document = pizzas_document();
        let selector = Selector::parse("pizzas[0]").unwrap();
        assert!(select_array(&document, &selector).is_err());
    }

    #[test]
    fn selecting_a_scalar_as_an_array_is_a_select_error() {
        let document = pizzas_document();
        let selector = Selector::parse("pizzas[0]/name").unwrap();
        assert!(select_array(&document, &selector).is_err());
    }

    #[test]
    fn array_missing_key_is_a_select_error() {
        let document = pizzas_document();
        let selector = Selector::parse("nope").unwrap();
        assert!(select_array(&document, &selector).is_err());
    }

    // -- Non-ASCII keys -------------------------------------------------

    #[test]
    fn a_key_in_any_script_parses() {
        for key in ["ピザ", "пицца", "피자", "żółw", "crème", "_ß", "日本語2"] {
            let selector = Selector::parse(&format!("{key}[0]/name")).unwrap();
            assert_eq!(
                selector.steps(),
                &[
                    Step::Key(key.to_string()),
                    Step::Index(0),
                    Step::Key("name".to_string()),
                ],
                "{key}"
            );
        }
    }

    #[test]
    fn a_key_with_a_combining_mark_parses_as_one_key() {
        let selector = Selector::parse("e\u{301}cole/x").unwrap();
        assert_eq!(
            selector.steps(),
            &[
                Step::Key("e\u{301}cole".to_string()),
                Step::Key("x".to_string()),
            ]
        );
    }

    #[test]
    fn a_non_ascii_key_selects_its_value() {
        let document: Value =
            serde_yaml_ng::from_str("メニュー:\n  - 名前: マルゲリータ\n  - 名前: ディアボロ\n")
                .unwrap();
        let selector = Selector::parse("メニュー[1]").unwrap();
        let selected = select_object(&document, &selector).unwrap();
        assert_eq!(selected["名前"].as_str(), Some("ディアボロ"));
    }

    #[test]
    fn an_emoji_in_a_selector_is_still_an_unexpected_character() {
        // Keys are letters, digits, and `_` (in any script) — an emoji
        // isn't a letter.
        let err = Selector::parse("🍕").unwrap_err();
        assert_eq!(
            err.to_string(),
            "selector \"🍕\" has an unexpected character `🍕`"
        );
    }

    #[test]
    fn a_key_cannot_start_with_a_digit_in_any_script() {
        // `٣` is ARABIC-INDIC DIGIT THREE; `１` is FULLWIDTH DIGIT ONE.
        assert!(Selector::parse("٣abc").is_err());
        assert!(Selector::parse("１abc").is_err());
    }

    #[test]
    fn a_non_ascii_digit_index_is_non_numeric() {
        let err = Selector::parse("pizzas[１]").unwrap_err();
        assert_eq!(
            err.to_string(),
            "selector \"pizzas[１]\" has a non-numeric index `[１]`"
        );
    }

    #[test]
    fn a_non_ascii_path_splits_from_its_selector() {
        let source = ObjectSource::parse("データ/ピザ 🍕.yaml#メニュー[0]").unwrap();
        assert_eq!(source.path, PathBuf::from("データ/ピザ 🍕.yaml"));
        assert_eq!(source.selector.unwrap().to_string(), "メニュー[0]");
    }

    #[test]
    fn an_out_of_range_index_names_the_sequences_length() {
        let document = pizzas_document();
        let one = serde_yaml_ng::from_str::<Value>("pizzas: [margherita]").unwrap();
        let err = select_object(&one, &Selector::parse("pizzas[1]").unwrap()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "selector \"pizzas[1]\": index [1] out of range (sequence has 1 element)"
        );
        let err = select_object(&document, &Selector::parse("pizzas[5]").unwrap()).unwrap_err();
        assert!(err.to_string().ends_with(" elements)"), "{err}");
    }
}
