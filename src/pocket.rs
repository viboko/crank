// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pocket markers: named regions of hand-written content embedded in a
//! generated file that survive a re-render by being read back from the
//! task group's previously generated output files (found via
//! `.crank.manifest`) — there's no separate persisted store; old content
//! is only ever recovered from files already on disk.
//!
//! Markers come in start/end pairs, each written as a [`MarkerPattern`]:
//! literal text with a `{name}` slot where the pocket's name goes. Two
//! pairs are built in and active at the same time — `// {name} [` ...
//! `// ]` and `/* {name} [ */` ... `/* ] */` — and a template can replace
//! them with its own list (see `header.rs`). A pocket's name is unique
//! across every pair together, and pockets from different pairs can't
//! nest or overlap, same as two pockets from the same pair can't.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// The slot in a [`MarkerPattern`]'s `start` where the pocket's name goes.
pub const NAME_PLACEHOLDER: &str = "{name}";

/// What a pocket's name may be: one or more letters, marks, digits or
/// connector punctuation (`\w`, Unicode-aware), or hyphens. Fixed, so a
/// pattern can move the name around but never change what it can be.
const NAME_REGEX: &str = r"(?P<name>[\w-]+)";

/// crank's built-in marker pairs, as `(start, end)` patterns.
const DEFAULT_PATTERNS: [(&str, &str); 2] =
    [("// {name} [", "// ]"), ("/* {name} [ */", "/* ] */")];

/// One start/end marker pair as written: literal text, where any run of
/// whitespace matches any amount of whitespace (including none), and
/// `start` holds exactly one `{name}`. The shape of each item in a
/// template header's `pocketMarkers` list and in a `.crank.manifest`
/// entry's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerPattern {
    /// The start marker, with a `{name}` slot.
    pub start: String,
    /// The end marker.
    pub end: String,
}

impl MarkerPattern {
    /// A pair from its `start` and `end` patterns.
    #[must_use]
    pub fn new(start: &str, end: &str) -> Self {
        Self {
            start: start.to_string(),
            end: end.to_string(),
        }
    }
}

/// crank's built-in marker pairs, `// {name} [` ... `// ]` and
/// `/* {name} [ */` ... `/* ] */` — what a template without its own
/// `pocketMarkers` uses.
#[must_use]
pub fn default_patterns() -> Vec<MarkerPattern> {
    DEFAULT_PATTERNS
        .iter()
        .map(|(start, end)| MarkerPattern::new(start, end))
        .collect()
}

/// One marker pair, compiled: a regex that finds a start marker (and
/// captures the pocket's name as `name`), and one that finds an end
/// marker. `label` is the pair's `start` pattern, so an overlap error can
/// say which pair each pocket came from.
#[derive(Debug, Clone)]
struct CompiledPair {
    /// The pair's `start` pattern, as written.
    label: String,
    /// Finds a start marker, capturing the pocket's name as `name`.
    start: Regex,
    /// Finds an end marker.
    end: Regex,
}

/// Every marker pair one file's pockets are written with, compiled. Built
/// with [`PocketMarkers::default_markers`] for a template without its own
/// `pocketMarkers`, or [`PocketMarkers::compile`] for one with them (and
/// for a `.crank.manifest` entry recording them).
#[derive(Debug, Clone)]
pub struct PocketMarkers {
    /// Every pair's matchers, in the order they were listed.
    pairs: Vec<CompiledPair>,
}

/// How often `{name}` appears in `pattern`.
fn name_slots(pattern: &str) -> usize {
    pattern.matches(NAME_PLACEHOLDER).count()
}

/// `pattern` as a regex: its text matched literally, any run of
/// whitespace as `\s*`, and `{name}` as [`NAME_REGEX`]. Whitespace at
/// either end is dropped, so a marker never swallows the line break
/// before or after it.
fn pattern_regex(pattern: &str) -> String {
    pattern
        .split_whitespace()
        .map(|word| {
            word.split(NAME_PLACEHOLDER)
                .map(regex::escape)
                .collect::<Vec<_>>()
                .join(NAME_REGEX)
        })
        .collect::<Vec<_>>()
        .join(r"\s*")
}

/// Checks one pair's patterns and compiles them — the single place a bad
/// pattern becomes a [`MarkerError`].
///
/// # Errors
/// Returns [`MarkerError`] if `start` doesn't hold exactly one `{name}`,
/// holds nothing else, or `end` is empty or holds a `{name}`.
fn compile_pair(pattern: &MarkerPattern) -> Result<CompiledPair, MarkerError> {
    let start = pattern.start.as_str();
    let end = pattern.end.as_str();
    if name_slots(start) != 1 {
        return Err(MarkerError::StartNeedsOneName(start.to_string()));
    }
    if start.replace(NAME_PLACEHOLDER, "").trim().is_empty() {
        return Err(MarkerError::StartIsOnlyName(start.to_string()));
    }
    if end.trim().is_empty() {
        return Err(MarkerError::EmptyEnd(start.to_string()));
    }
    if name_slots(end) != 0 {
        return Err(MarkerError::EndHasName(end.to_string()));
    }
    let compile = |pattern: &str| {
        Regex::new(&pattern_regex(pattern))
            .expect("escaped text joined by `\\s*` and one name group is always a valid regex")
    };
    Ok(CompiledPair {
        label: start.to_string(),
        start: compile(start),
        end: compile(end),
    })
}

impl PocketMarkers {
    /// Compiles `patterns`, which replace crank's built-in pairs entirely.
    /// An empty list is allowed: a file written with it has no pockets.
    ///
    /// # Errors
    /// Returns the first [`MarkerError`] among `patterns`.
    pub fn compile(patterns: &[MarkerPattern]) -> Result<Self, MarkerError> {
        let pairs = patterns
            .iter()
            .map(compile_pair)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { pairs })
    }

    /// crank's built-in marker pairs (see [`default_patterns`]), compiled.
    ///
    /// # Panics
    /// Never: the built-in patterns are valid, which the tests check.
    #[must_use]
    pub fn default_markers() -> Self {
        Self::compile(&default_patterns()).expect("crank's built-in pocket markers are valid")
    }
}

/// A marker pair that can't be used: `start` without exactly one `{name}`,
/// or with nothing but it, or an empty `end`, or one with a `{name}`.
#[derive(Debug, PartialEq, Eq)]
pub enum MarkerError {
    /// A `start` (given) with no `{name}`, or more than one.
    StartNeedsOneName(String),
    /// A `start` (given) that's only `{name}`, which would make every
    /// word a pocket.
    StartIsOnlyName(String),
    /// The pair with this `start` has an empty `end`.
    EmptyEnd(String),
    /// An `end` (given) with a `{name}` in it.
    EndHasName(String),
}

impl fmt::Display for MarkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarkerError::StartNeedsOneName(start) => write!(
                f,
                "the start marker \"{start}\" must contain `{NAME_PLACEHOLDER}` exactly once, where the pocket's name goes"
            ),
            MarkerError::StartIsOnlyName(start) => write!(
                f,
                "the start marker \"{start}\" needs some text besides `{NAME_PLACEHOLDER}`, or every word would start a pocket"
            ),
            MarkerError::EmptyEnd(start) => {
                write!(f, "the end marker paired with \"{start}\" is empty")
            }
            MarkerError::EndHasName(end) => write!(
                f,
                "the end marker \"{end}\" can't contain `{NAME_PLACEHOLDER}` — only a start marker names its pocket"
            ),
        }
    }
}

impl std::error::Error for MarkerError {}

/// One pocket marker pair found while scanning a piece of text — byte
/// ranges into that same text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PocketMatch {
    /// The pocket's name, from its start marker.
    name: String,
    /// The start marker itself.
    start_marker: Range<usize>,
    /// The content between the two markers.
    body: Range<usize>,
    /// The end marker itself.
    end_marker: Range<usize>,
}

/// A pocket marker pair that's malformed, or a naming/positioning
/// conflict between pockets: an unclosed start marker, a stray end
/// marker, a name reused elsewhere in the same task group's fresh output,
/// a name found in more than one of the task group's previous output
/// files, or two pockets (from the same or different marker pairs) whose
/// markers overlap.
#[derive(Debug)]
pub enum PocketError {
    /// A start marker has no matching end marker.
    UnclosedPocket {
        /// The unclosed pocket's name.
        name: String,
    },
    /// An end marker with no start marker before it.
    StrayEndMarker,
    /// A name used by more than one pocket in the task group's fresh
    /// output.
    DuplicatePocketName {
        /// The repeated name.
        name: String,
    },
    /// A pocket this run needs was found in more than one previous
    /// output file, so which old content to keep is ambiguous.
    AmbiguousOldContent {
        /// The pocket's name.
        name: String,
        /// Every previous output file it was found in.
        files: Vec<PathBuf>,
    },
    /// Two pockets' markers overlap.
    OverlappingMarkers {
        /// The `start` pattern of the first pocket's marker pair.
        first_marker: String,
        /// The first pocket's name.
        first_name: String,
        /// The `start` pattern of the second pocket's marker pair.
        second_marker: String,
        /// The second pocket's name.
        second_name: String,
    },
}

impl fmt::Display for PocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PocketError::UnclosedPocket { name } => write!(
                f,
                "pocket \"{name}\" is never closed — its start marker has no matching end marker before end of file or before another pocket's start marker (pockets can't nest)"
            ),
            PocketError::StrayEndMarker => write!(
                f,
                "found a pocket end marker with no preceding start marker"
            ),
            PocketError::DuplicatePocketName { name } => write!(
                f,
                "pocket \"{name}\" is used more than once in this run's generated output — pocket names must be unique across the whole task group, whichever markers it uses"
            ),
            PocketError::AmbiguousOldContent { name, files } => {
                let list = files
                    .iter()
                    .map(|file| file.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "pocket \"{name}\" was found in more than one previously generated file ({list}) — can't tell which hand-edited content to keep"
                )
            }
            PocketError::OverlappingMarkers {
                first_marker,
                first_name,
                second_marker,
                second_name,
            } => write!(
                f,
                "pocket \"{first_name}\" (marked \"{first_marker}\") overlaps with pocket \"{second_name}\" (marked \"{second_marker}\") — pockets can't nest or overlap, even with different markers"
            ),
        }
    }
}

impl std::error::Error for PocketError {}

/// A single-pass walk over every start/end marker `pair` finds in `text`,
/// merged and sorted by position, pairing each start with the next end.
/// Pockets don't nest: a second start marker before the first closes, or
/// an end marker with nothing open, is an error. A marker that begins
/// inside one already taken — an end pattern that also matches the tail
/// of its own start marker, say — is part of that one, not a marker of
/// its own; where two begin at the same place, the longer one is taken.
///
/// # Errors
/// Returns [`PocketError::UnclosedPocket`] or
/// [`PocketError::StrayEndMarker`].
fn scan_pair(text: &str, pair: &CompiledPair) -> Result<Vec<PocketMatch>, PocketError> {
    enum Kind {
        Start(String),
        End,
    }

    let mut tokens: Vec<(usize, usize, Kind)> = Vec::new();
    for caps in pair.start.captures_iter(text) {
        let whole = caps.get(0).expect("the whole match is always present");
        let name = caps
            .name("name")
            .expect("compile_pair guarantees a `name` capture group")
            .as_str()
            .to_string();
        tokens.push((whole.start(), whole.end(), Kind::Start(name)));
    }
    for found in pair.end.find_iter(text) {
        tokens.push((found.start(), found.end(), Kind::End));
    }
    tokens.sort_by_key(|&(start, end, _)| (start, std::cmp::Reverse(end)));

    let mut matches = Vec::new();
    let mut open: Option<(String, usize, usize)> = None;
    let mut taken = 0;
    for (start, end, kind) in tokens {
        if start < taken {
            continue;
        }
        taken = end;
        match (kind, open.take()) {
            (Kind::Start(name), None) => open = Some((name, start, end)),
            (Kind::Start(_), Some((open_name, ..))) => {
                return Err(PocketError::UnclosedPocket { name: open_name });
            }
            (Kind::End, None) => return Err(PocketError::StrayEndMarker),
            (Kind::End, Some((name, marker_start, marker_end))) => {
                matches.push(PocketMatch {
                    name,
                    start_marker: marker_start..marker_end,
                    body: marker_end..start,
                    end_marker: start..end,
                });
            }
        }
    }
    if let Some((name, ..)) = open {
        return Err(PocketError::UnclosedPocket { name });
    }
    Ok(matches)
}

/// Runs [`scan_pair`] for every marker pair, then merges all pairs'
/// matches into one document-order stream. Each pair's own matches are
/// already non-overlapping and sorted (guaranteed by `scan_pair`'s
/// no-nesting rule), so after merging and sorting all pairs' matches by
/// start position, checking only *adjacent* matches is enough to detect
/// any overlap between any two, from the same or different pairs: if two
/// matches overlap, at least one of them overlaps whatever match sorts
/// immediately next to it. Overlap is checked on the whole match range
/// (start marker through end marker), so it catches both partial overlap
/// and one pair's marker sitting entirely inside another pocket's body —
/// the same rule as nesting within one pair, just applied across pairs
/// too.
///
/// # Errors
/// Returns [`PocketError::UnclosedPocket`]/[`PocketError::StrayEndMarker`]
/// if any pair finds a malformed marker in `text`, or
/// [`PocketError::OverlappingMarkers`] if two matches (from the same or
/// different pairs) overlap.
fn scan(text: &str, markers: &PocketMarkers) -> Result<Vec<PocketMatch>, PocketError> {
    let mut tagged: Vec<(PocketMatch, &str)> = Vec::new();
    for pair in &markers.pairs {
        for found in scan_pair(text, pair)? {
            tagged.push((found, pair.label.as_str()));
        }
    }
    tagged.sort_by_key(|(found, _)| found.start_marker.start);

    for ((first, first_marker), (second, second_marker)) in tagged.iter().zip(tagged.iter().skip(1))
    {
        if first.end_marker.end > second.start_marker.start {
            return Err(PocketError::OverlappingMarkers {
                first_marker: (*first_marker).to_string(),
                first_name: first.name.clone(),
                second_marker: (*second_marker).to_string(),
                second_name: second.name.clone(),
            });
        }
    }

    Ok(tagged.into_iter().map(|(found, _)| found).collect())
}

/// One pocket found while scanning a task group's previously generated
/// output files: which file it came from, and its current (possibly
/// hand-edited) content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OldPocket {
    /// The previous output file it was found in.
    pub file: PathBuf,
    /// What's between its markers there now.
    pub content: String,
}

/// Every pocket found across a task group's previously generated output
/// files, keyed by name. More than one entry for a name means it's
/// ambiguous which old content to use — only surfaced as an error by
/// [`apply`] if this run's fresh render actually needs that name; if it
/// doesn't, it's just an orphan, dropped silently like any other.
pub type OldPockets = HashMap<String, Vec<OldPocket>>;

/// Scans every already-read file in `files` (`(path, contents, markers)`
/// triples — this function does no I/O itself) for pocket marker pairs,
/// merging them into one name -> occurrences map for the whole task group.
/// Each file carries its own `markers`, rather than one set shared across
/// the whole call, because different files in the same task group can
/// have been written from templates with different `pocketMarkers` — the
/// caller (see `run.rs::load_old_pockets`) takes each file's markers from
/// whatever `.crank.manifest` recorded for it, independent of what its
/// template says *now*. Collects every file's scan error rather than
/// stopping at the first, mirroring `run.rs::load_filters`'s "report
/// every broken filter, not just the first" style.
///
/// # Errors
/// Returns one `(path, `[`PocketError`]`)` pair per file that failed to
/// scan.
pub fn collect_old_pockets(
    files: &[(PathBuf, String, &PocketMarkers)],
) -> Result<OldPockets, Vec<(PathBuf, PocketError)>> {
    let mut old_pockets: OldPockets = HashMap::new();
    let mut errors = Vec::new();

    for (path, contents, markers) in files {
        match scan(contents, markers) {
            Ok(matches) => {
                for found in matches {
                    old_pockets.entry(found.name).or_default().push(OldPocket {
                        file: path.clone(),
                        content: contents[found.body].to_string(),
                    });
                }
            }
            Err(err) => errors.push((path.clone(), err)),
        }
    }

    if errors.is_empty() {
        Ok(old_pockets)
    } else {
        Err(errors)
    }
}

/// Re-applies `old_pockets` to `fresh_text` — one task/item's just-
/// rendered output, before it's written to disk. Finds every pocket
/// marker pair in `fresh_text` (in document order, across every one of
/// `markers`' pairs) and, for each one:
///
/// - Records its name in `seen` (shared across every task, and, for an
///   array group, every item, in the same task group, since pocket names
///   are unique across the whole group, not per task or per marker pair); a name
///   already present is [`PocketError::DuplicatePocketName`].
/// - Looks it up in `old_pockets`: no match keeps the freshly-rendered
///   default body; exactly one match substitutes that old content in
///   place of the fresh body (the fresh render's marker lines are always
///   kept); more than one match is [`PocketError::AmbiguousOldContent`].
///
/// # Errors
/// Returns [`PocketError::UnclosedPocket`]/[`PocketError::StrayEndMarker`]
/// if `fresh_text` itself has malformed markers,
/// [`PocketError::OverlappingMarkers`] if two of its markers overlap,
/// [`PocketError::DuplicatePocketName`], or
/// [`PocketError::AmbiguousOldContent`].
// `seen` is always the std `HashSet` crank builds internally — this is a
// binary crate's private wiring, not a library API other crates hand
// arbitrary hashers to, so genericizing over `BuildHasher` buys nothing.
#[allow(clippy::implicit_hasher)]
pub fn apply(
    fresh_text: &str,
    old_pockets: &OldPockets,
    seen: &mut HashSet<String>,
    markers: &PocketMarkers,
) -> Result<String, PocketError> {
    let matches = scan(fresh_text, markers)?;
    let mut result = String::with_capacity(fresh_text.len());
    let mut cursor = 0;

    for found in &matches {
        if !seen.insert(found.name.clone()) {
            return Err(PocketError::DuplicatePocketName {
                name: found.name.clone(),
            });
        }

        result.push_str(&fresh_text[cursor..found.start_marker.end]);

        match old_pockets.get(&found.name).map(Vec::as_slice) {
            None | Some([]) => result.push_str(&fresh_text[found.body.clone()]),
            Some([single]) => result.push_str(&single.content),
            Some(many) => {
                return Err(PocketError::AmbiguousOldContent {
                    name: found.name.clone(),
                    files: many.iter().map(|old| old.file.clone()).collect(),
                });
            }
        }

        cursor = found.end_marker.start;
    }
    result.push_str(&fresh_text[cursor..]);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built_in() -> PocketMarkers {
        PocketMarkers::default_markers()
    }

    /// `patterns`, as `(start, end)` pairs, compiled.
    fn markers(patterns: &[(&str, &str)]) -> PocketMarkers {
        let patterns: Vec<MarkerPattern> = patterns
            .iter()
            .map(|(start, end)| MarkerPattern::new(start, end))
            .collect();
        PocketMarkers::compile(&patterns).unwrap()
    }

    /// The names of the pockets `markers` finds in `text`.
    fn names(text: &str, markers: &PocketMarkers) -> Vec<String> {
        scan(text, markers)
            .unwrap()
            .into_iter()
            .map(|found| found.name)
            .collect()
    }

    #[test]
    fn the_built_in_markers_are_two_pairs() {
        assert_eq!(built_in().pairs.len(), 2);
        assert_eq!(
            default_patterns(),
            vec![
                MarkerPattern::new("// {name} [", "// ]"),
                MarkerPattern::new("/* {name} [ */", "/* ] */"),
            ]
        );
    }

    #[test]
    fn a_pattern_compiles_to_its_text_with_whitespace_as_any_amount() {
        assert_eq!(pattern_regex("// {name} ["), r"//\s*(?P<name>[\w-]+)\s*\[");
        assert_eq!(
            pattern_regex("/* {name} [ */"),
            r"/\*\s*(?P<name>[\w-]+)\s*\[\s*\*/"
        );
        assert_eq!(pattern_regex("  <!--\t]  -->\n"), r"<!\-\-\s*\]\s*\-\->");
    }

    #[test]
    fn a_pattern_without_whitespace_needs_none_in_the_text() {
        let markers = markers(&[("#{name}[", "#]")]);
        assert_eq!(names("#a[\nx\n#]\n", &markers), vec!["a"]);
        assert_eq!(names("# b [\nx\n# ]\n", &markers), Vec::<String>::new());
    }

    #[test]
    fn whitespace_in_a_pattern_also_matches_none() {
        let markers = markers(&[("# {name} [", "# ]")]);
        assert_eq!(names("#a[\nx\n#]\n", &markers), vec!["a"]);
    }

    #[test]
    fn regex_syntax_in_a_pattern_is_matched_literally() {
        let markers = markers(&[("(.*) {name} [", "(.*) ]")]);
        assert_eq!(names("(.*) a [\nx\n(.*) ]\n", &markers), vec!["a"]);
        assert_eq!(names("xx a [\nx\nxx ]\n", &markers), Vec::<String>::new());
    }

    #[test]
    fn a_pattern_can_put_the_name_anywhere_but_not_change_what_it_can_be() {
        let markers = markers(&[("<!-- [ {name} -->", "<!-- ] -->")]);
        assert_eq!(
            names("<!-- [ メモ-2 -->x<!-- ] -->", &markers),
            vec!["メモ-2"]
        );
        // A name is still letters, digits, `_` and `-`: a `.` isn't one.
        assert!(scan("<!-- [ a.b -->x<!-- ] -->", &markers).is_err());
    }

    #[test]
    fn listed_markers_replace_the_built_in_ones_entirely() {
        let markers = markers(&[("# {name} [", "# ]")]);
        let text = "# a [\nx\n# ]\n// b [\ny\n// ]\n/* c [ */z/* ] */\n";
        assert_eq!(names(text, &markers), vec!["a"]);
    }

    #[test]
    fn floor_division_in_python_isnt_a_pocket_once_the_built_in_markers_are_replaced() {
        let python = "x = a // b[0]\n";
        // With the built-in markers, `// b[` starts a pocket named "b"...
        assert!(matches!(
            scan(python, &built_in()),
            Err(PocketError::UnclosedPocket { name }) if name == "b"
        ));
        // ...but not once a template lists its own.
        let markers = markers(&[("# {name} [", "# ]")]);
        assert_eq!(names(python, &markers), Vec::<String>::new());
    }

    #[test]
    fn several_listed_pairs_are_all_active() {
        let markers = markers(&[("# {name} [", "# ]"), ("<!-- {name} [ -->", "<!-- ] -->")]);
        let text = "# a [\nfirst\n# ]\n<!-- b [ -->second<!-- ] -->\n";
        assert_eq!(names(text, &markers), vec!["a", "b"]);
    }

    #[test]
    fn an_empty_list_means_no_pockets() {
        let markers = markers(&[]);
        assert_eq!(names("// a [\nx\n// ]\n", &markers), Vec::<String>::new());
    }

    #[test]
    fn an_end_pattern_matching_the_tail_of_its_start_marker_doesnt_close_it() {
        // `>` also ends the start marker `<a>`: that `>` belongs to it.
        let markers = markers(&[("< {name} >", ">")]);
        let mut seen = HashSet::new();
        let result = apply("<a>body>", &OldPockets::new(), &mut seen, &markers).unwrap();
        assert_eq!(result, "<a>body>");
        assert!(seen.contains("a"));
    }

    #[test]
    fn an_end_pattern_matching_the_head_of_its_start_marker_doesnt_close_anything() {
        // `#` ends a pocket, and also begins every start marker.
        let markers = markers(&[("# {name} [", "#")]);
        let old =
            collect_old_pockets(&[(PathBuf::from("old.txt"), "# a [kept#".to_string(), &markers)])
                .unwrap();
        assert_eq!(old["a"][0].content, "kept");
    }

    #[test]
    fn a_start_pattern_without_a_name_is_an_error() {
        let pattern = MarkerPattern::new("# [", "# ]");
        assert_eq!(
            PocketMarkers::compile(&[pattern]).unwrap_err(),
            MarkerError::StartNeedsOneName("# [".to_string())
        );
    }

    #[test]
    fn a_start_pattern_with_two_names_is_an_error() {
        let pattern = MarkerPattern::new("# {name} {name} [", "# ]");
        assert!(matches!(
            PocketMarkers::compile(&[pattern]),
            Err(MarkerError::StartNeedsOneName(_))
        ));
    }

    #[test]
    fn a_start_pattern_of_only_a_name_is_an_error() {
        let pattern = MarkerPattern::new(" {name} ", "]");
        assert!(matches!(
            PocketMarkers::compile(&[pattern]),
            Err(MarkerError::StartIsOnlyName(_))
        ));
    }

    #[test]
    fn an_empty_end_pattern_is_an_error() {
        let pattern = MarkerPattern::new("# {name} [", "  ");
        assert_eq!(
            PocketMarkers::compile(&[pattern]).unwrap_err(),
            MarkerError::EmptyEnd("# {name} [".to_string())
        );
    }

    #[test]
    fn an_end_pattern_with_a_name_is_an_error() {
        let pattern = MarkerPattern::new("# {name} [", "# {name} ]");
        assert!(matches!(
            PocketMarkers::compile(&[pattern]),
            Err(MarkerError::EndHasName(_))
        ));
    }

    #[test]
    fn the_first_bad_pair_is_the_one_reported() {
        let patterns = [
            MarkerPattern::new("# {name} [", "# ]"),
            MarkerPattern::new("-- [", "-- ]"),
            MarkerPattern::new("; [", "; ]"),
        ];
        assert_eq!(
            PocketMarkers::compile(&patterns).unwrap_err(),
            MarkerError::StartNeedsOneName("-- [".to_string())
        );
    }

    #[test]
    fn marker_errors_say_what_to_fix() {
        assert_eq!(
            MarkerError::StartNeedsOneName("# [".to_string()).to_string(),
            "the start marker \"# [\" must contain `{name}` exactly once, where the pocket's name goes"
        );
        assert_eq!(
            MarkerError::EndHasName("{name}".to_string()).to_string(),
            "the end marker \"{name}\" can't contain `{name}` — only a start marker names its pocket"
        );
    }

    #[test]
    fn an_unrecognized_key_in_a_marker_pattern_is_a_parse_error() {
        let source = "start: '# {name} ['\nend: '# ]'\nmiddle: x\n";
        assert!(serde_yaml_ng::from_str::<MarkerPattern>(source).is_err());
    }

    #[test]
    fn collect_old_pockets_scans_each_file_with_its_own_markers() {
        let default = built_in();
        let hash = markers(&[("# {name} [", "# ]")]);

        // File a was written with the default `//` markers; file b with a
        // template's own `#` ones — each should only be recognized under
        // its own markers, not the other's.
        let file_a = "// from-a [\nfirst\n// ]\n";
        let file_b = "# from-b [\nsecond\n# ]\n";
        let old_pockets = collect_old_pockets(&[
            (PathBuf::from("a.txt"), file_a.to_string(), &default),
            (PathBuf::from("b.txt"), file_b.to_string(), &hash),
        ])
        .unwrap();

        assert!(old_pockets.contains_key("from-a"));
        assert!(old_pockets.contains_key("from-b"));
    }

    #[test]
    fn overlapping_markers_name_each_pockets_start_pattern() {
        let fresh = "// outer [\n/* inner [ */content/* ] */\n// ]\n";
        let err = scan(fresh, &built_in()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "pocket \"outer\" (marked \"// {name} [\") overlaps with pocket \"inner\" (marked \"/* {name} [ */\") — pockets can't nest or overlap, even with different markers"
        );
    }

    #[test]
    fn first_run_with_no_old_pockets_keeps_the_fresh_default_content() {
        let fresh = "before\n// custom [\ndefault content\n// ]\nafter\n";
        let mut seen = HashSet::new();
        let result = apply(fresh, &OldPockets::new(), &mut seen, &built_in()).unwrap();
        assert_eq!(result, fresh);
    }

    #[test]
    fn old_content_is_substituted_for_the_fresh_default() {
        let fresh = "before\n// custom [\ndefault content\n// ]\nafter\n";
        let mut old_pockets = OldPockets::new();
        old_pockets.insert(
            "custom".to_string(),
            vec![OldPocket {
                file: PathBuf::from("old.txt"),
                content: "\nhand-edited content\n".to_string(),
            }],
        );
        let mut seen = HashSet::new();
        let result = apply(fresh, &old_pockets, &mut seen, &built_in()).unwrap();
        assert_eq!(
            result,
            "before\n// custom [\nhand-edited content\n// ]\nafter\n"
        );
    }

    #[test]
    fn a_pocket_found_in_a_different_old_file_than_the_fresh_one_still_substitutes() {
        let old_file_a = "// custom [\nfrom file a\n// ]\n";
        let old_file_b = "// other [\nfrom file b\n// ]\n";
        let cfg = built_in();
        let old_pockets = collect_old_pockets(&[
            (PathBuf::from("a.txt"), old_file_a.to_string(), &cfg),
            (PathBuf::from("b.txt"), old_file_b.to_string(), &cfg),
        ])
        .unwrap();

        // The template was restructured: "custom" now lives in a file that
        // didn't exist last run.
        let fresh = "// custom [\nnew default\n// ]\n";
        let mut seen = HashSet::new();
        let result = apply(fresh, &old_pockets, &mut seen, &built_in()).unwrap();
        assert_eq!(result, "// custom [\nfrom file a\n// ]\n");
    }

    #[test]
    fn an_orphaned_old_pocket_is_silently_dropped() {
        let mut old_pockets = OldPockets::new();
        old_pockets.insert(
            "gone".to_string(),
            vec![OldPocket {
                file: PathBuf::from("old.txt"),
                content: "\nold content\n".to_string(),
            }],
        );
        let fresh = "no pockets here\n";
        let mut seen = HashSet::new();
        let result = apply(fresh, &old_pockets, &mut seen, &built_in()).unwrap();
        assert_eq!(result, fresh);
    }

    #[test]
    fn a_duplicate_pocket_name_within_one_fresh_render_is_an_error() {
        let fresh = "// dup [\na\n// ]\n// dup [\nb\n// ]\n";
        let mut seen = HashSet::new();
        assert!(matches!(
            apply(fresh, &OldPockets::new(), &mut seen, &built_in()),
            Err(PocketError::DuplicatePocketName { name }) if name == "dup"
        ));
    }

    #[test]
    fn a_duplicate_pocket_name_across_two_apply_calls_sharing_seen_is_an_error() {
        let mut seen = HashSet::new();
        let first = "// dup [\na\n// ]\n";
        let second = "// dup [\nb\n// ]\n";
        apply(first, &OldPockets::new(), &mut seen, &built_in()).unwrap();
        assert!(matches!(
            apply(second, &OldPockets::new(), &mut seen, &built_in()),
            Err(PocketError::DuplicatePocketName { name }) if name == "dup"
        ));
    }

    #[test]
    fn an_unclosed_pocket_at_eof_is_an_error() {
        let fresh = "// x [\nno closing marker\n";
        let mut seen = HashSet::new();
        assert!(matches!(
            apply(fresh, &OldPockets::new(), &mut seen, &built_in()),
            Err(PocketError::UnclosedPocket { name }) if name == "x"
        ));
    }

    #[test]
    fn an_unclosed_pocket_before_another_start_marker_is_an_error() {
        let fresh = "// x [\n// y [\ncontent\n// ]\n";
        let mut seen = HashSet::new();
        assert!(matches!(
            apply(fresh, &OldPockets::new(), &mut seen, &built_in()),
            Err(PocketError::UnclosedPocket { name }) if name == "x"
        ));
    }

    #[test]
    fn a_stray_end_marker_is_an_error() {
        let fresh = "no start here\n// ]\n";
        let mut seen = HashSet::new();
        assert!(matches!(
            apply(fresh, &OldPockets::new(), &mut seen, &built_in()),
            Err(PocketError::StrayEndMarker)
        ));
    }

    #[test]
    fn whitespace_variations_between_marker_tokens_all_match() {
        let fresh = "//   spaced   [\ncontent\n//     ]\n";
        let mut seen = HashSet::new();
        let result = apply(fresh, &OldPockets::new(), &mut seen, &built_in()).unwrap();
        assert_eq!(result, fresh);
        assert!(seen.contains("spaced"));
    }

    #[test]
    fn ambiguous_old_content_across_two_files_is_an_error() {
        let cfg = built_in();
        let old_pockets = collect_old_pockets(&[
            (
                PathBuf::from("a.txt"),
                "// dup [\nfrom a\n// ]\n".to_string(),
                &cfg,
            ),
            (
                PathBuf::from("b.txt"),
                "// dup [\nfrom b\n// ]\n".to_string(),
                &cfg,
            ),
        ])
        .unwrap();
        let fresh = "// dup [\ndefault\n// ]\n";
        let mut seen = HashSet::new();
        assert!(matches!(
            apply(fresh, &old_pockets, &mut seen, &built_in()),
            Err(PocketError::AmbiguousOldContent { name, files })
                if name == "dup" && files.len() == 2
        ));
    }

    #[test]
    fn collect_old_pockets_gathers_every_files_scan_error_not_just_the_first() {
        let cfg = built_in();
        let files = vec![
            (PathBuf::from("a.txt"), "// unclosed [\n".to_string(), &cfg),
            (PathBuf::from("b.txt"), "stray\n// ]\n".to_string(), &cfg),
        ];
        let errors = collect_old_pockets(&files).unwrap_err();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn apply_never_mutates_seen_with_the_current_pockets_content_only_its_name() {
        let fresh = "// notes [\nsome content\n// ]\n";
        let mut seen = HashSet::new();
        apply(fresh, &OldPockets::new(), &mut seen, &built_in()).unwrap();
        assert_eq!(seen, HashSet::from(["notes".to_string()]));
    }

    #[test]
    fn short_form_first_run_with_no_old_pockets_keeps_the_fresh_default_content() {
        let fresh = "before /* notes [ */default content/* ] */ after\n";
        let mut seen = HashSet::new();
        let result = apply(fresh, &OldPockets::new(), &mut seen, &built_in()).unwrap();
        assert_eq!(result, fresh);
    }

    #[test]
    fn short_form_old_content_is_substituted_for_the_fresh_default() {
        let fresh = "before /* notes [ */default/* ] */ after\n";
        let mut old_pockets = OldPockets::new();
        old_pockets.insert(
            "notes".to_string(),
            vec![OldPocket {
                file: PathBuf::from("old.txt"),
                content: "hand-edited".to_string(),
            }],
        );
        let mut seen = HashSet::new();
        let result = apply(fresh, &old_pockets, &mut seen, &built_in()).unwrap();
        assert_eq!(result, "before /* notes [ */hand-edited/* ] */ after\n");
    }

    #[test]
    fn short_form_pocket_can_span_multiple_lines() {
        let fresh = "/* notes [ */\nline one\nline two\n/* ] */\n";
        let mut seen = HashSet::new();
        let result = apply(fresh, &OldPockets::new(), &mut seen, &built_in()).unwrap();
        assert_eq!(result, fresh);
    }

    #[test]
    fn long_form_and_short_form_coexist_in_the_same_text() {
        let fresh = "// notes [\nline default\n// ]\n/* tuning [ */42/* ] */\n";
        let mut old_pockets = OldPockets::new();
        old_pockets.insert(
            "tuning".to_string(),
            vec![OldPocket {
                file: PathBuf::from("old.txt"),
                content: "99".to_string(),
            }],
        );
        let mut seen = HashSet::new();
        let result = apply(fresh, &old_pockets, &mut seen, &built_in()).unwrap();
        assert_eq!(
            result,
            "// notes [\nline default\n// ]\n/* tuning [ */99/* ] */\n"
        );
        assert_eq!(
            seen,
            HashSet::from(["notes".to_string(), "tuning".to_string()])
        );
    }

    #[test]
    fn duplicate_pocket_name_across_different_forms_is_an_error() {
        let fresh = "// dup [\na\n// ]\n/* dup [ */b/* ] */\n";
        let mut seen = HashSet::new();
        assert!(matches!(
            apply(fresh, &OldPockets::new(), &mut seen, &built_in()),
            Err(PocketError::DuplicatePocketName { name }) if name == "dup"
        ));
    }

    #[test]
    fn overlapping_markers_from_different_forms_is_an_error() {
        let fresh = "// outer [\n/* inner [ */content/* ] */\n// ]\n";
        let mut seen = HashSet::new();
        assert!(matches!(
            apply(fresh, &OldPockets::new(), &mut seen, &built_in()),
            Err(PocketError::OverlappingMarkers { first_name, second_name, .. })
                if first_name == "outer" && second_name == "inner"
        ));
    }

    #[test]
    fn an_unclosed_short_form_pocket_is_an_error() {
        let fresh = "/* x [ */no closing marker\n";
        let mut seen = HashSet::new();
        assert!(matches!(
            apply(fresh, &OldPockets::new(), &mut seen, &built_in()),
            Err(PocketError::UnclosedPocket { name }) if name == "x"
        ));
    }

    #[test]
    fn a_stray_short_form_end_marker_is_an_error() {
        let fresh = "no start here /* ] */\n";
        let mut seen = HashSet::new();
        assert!(matches!(
            apply(fresh, &OldPockets::new(), &mut seen, &built_in()),
            Err(PocketError::StrayEndMarker)
        ));
    }

    #[test]
    fn whitespace_variations_in_short_form_markers_all_match() {
        let fresh = "/*   spaced   [   */content/*     ]     */\n";
        let mut seen = HashSet::new();
        let result = apply(fresh, &OldPockets::new(), &mut seen, &built_in()).unwrap();
        assert_eq!(result, fresh);
        assert!(seen.contains("spaced"));
    }

    // -- Non-ASCII text ---------------------------------------------------

    #[test]
    fn a_pocket_name_in_any_script_is_recognized() {
        for name in [
            "メモ",
            "заметки",
            "메모",
            "notatki-żółw",
            "crème_brûlée",
            "e\u{301}cole",
            "日本語-2",
        ] {
            let fresh = format!("// {name} [\ndefault\n// ]\n/* {name}-inline [ */x/* ] */\n");
            let mut old = OldPockets::new();
            for (pocket, content) in [
                (name.to_string(), "\nkept\n"),
                (format!("{name}-inline"), "y"),
            ] {
                old.insert(
                    pocket,
                    vec![OldPocket {
                        file: PathBuf::from("old.txt"),
                        content: content.to_string(),
                    }],
                );
            }
            let mut seen = HashSet::new();
            let result = apply(&fresh, &old, &mut seen, &built_in()).unwrap();
            assert_eq!(
                result,
                format!("// {name} [\nkept\n// ]\n/* {name}-inline [ */y/* ] */\n"),
                "{name}"
            );
        }
    }

    #[test]
    fn an_emoji_is_not_a_pocket_name_character() {
        // Like other punctuation, an emoji in the name position means the
        // start marker doesn't match — so its end marker is a stray one.
        let mut seen = HashSet::new();
        assert!(matches!(
            apply(
                "// 🍕 [\nx\n// ]\n",
                &OldPockets::new(),
                &mut seen,
                &built_in()
            ),
            Err(PocketError::StrayEndMarker)
        ));
    }

    #[test]
    fn multi_byte_text_around_and_inside_markers_is_preserved() {
        let fresh =
            "前置き🍕// a [\nデフォルト 👩‍👩‍👧‍👦\n// ]後書き🇵🇱\nZażółć/* b [ */ gęślą /* ] */jaźń\n";
        let old_text = "全然違う\n// a [\nПривет e\u{301}\n// ]\n/* b [ */ 김치 /* ] */\n";
        let old =
            collect_old_pockets(&[(PathBuf::from("old.txt"), old_text.to_string(), &built_in())])
                .unwrap();
        let mut seen = HashSet::new();
        let result = apply(fresh, &old, &mut seen, &built_in()).unwrap();
        assert_eq!(
            result,
            "前置き🍕// a [\nПривет e\u{301}\n// ]後書き🇵🇱\nZażółć/* b [ */ 김치 /* ] */jaźń\n"
        );
    }

    #[test]
    fn unicode_whitespace_between_marker_tokens_is_allowed() {
        // `\s` is Unicode-aware: an ideographic space (U+3000) or a
        // no-break space (U+00A0) counts as whitespace.
        let fresh = "//\u{3000}メモ\u{3000}[\nx\n//\u{a0}]\n";
        let mut seen = HashSet::new();
        let result = apply(fresh, &OldPockets::new(), &mut seen, &built_in()).unwrap();
        assert_eq!(result, fresh);
        assert!(seen.contains("メモ"));
    }

    #[test]
    fn a_duplicate_non_ascii_pocket_name_is_reported_by_name() {
        let fresh = "// メモ [\na\n// ]\n// メモ [\nb\n// ]\n";
        let mut seen = HashSet::new();
        let err = apply(fresh, &OldPockets::new(), &mut seen, &built_in()).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("pocket \"メモ\" is used more than once")
        );
    }

    #[test]
    fn crlf_line_endings_round_trip_inside_a_pocket() {
        let fresh = "// メモ [\r\nデフォルト\r\n// ]\r\n";
        let old = collect_old_pockets(&[(
            PathBuf::from("old.txt"),
            "// メモ [\r\n編集済み ✓\r\n// ]\r\n".to_string(),
            &built_in(),
        )])
        .unwrap();
        let mut seen = HashSet::new();
        let result = apply(fresh, &old, &mut seen, &built_in()).unwrap();
        assert_eq!(result, "// メモ [\r\n編集済み ✓\r\n// ]\r\n");
    }

    #[test]
    fn pockets_directly_next_to_each_other_dont_overlap() {
        let fresh = "/* a [ */x/* ] *//* b [ */y/* ] */\n";
        let mut seen = HashSet::new();
        let result = apply(fresh, &OldPockets::new(), &mut seen, &built_in()).unwrap();
        assert_eq!(result, fresh);
    }
}
