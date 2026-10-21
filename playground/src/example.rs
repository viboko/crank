// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The playground's examples: one Markdown file each in `examples/`, read
//! by people on GitHub, loaded into the playground by its example buttons,
//! and checked by this crate's tests to produce exactly the output and
//! files they show. See `playground/README.md` for the format.

use serde::Serialize;

use crate::TreeEntry;

/// One example, parsed from its Markdown file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Example {
    /// The file name without `.md`, e.g. `01-a-first-template`.
    pub id: String,
    /// The `# ` heading.
    pub title: String,
    /// The Markdown between the title and the first `## ` section.
    pub explanation: String,
    /// Every file the example starts with, with its contents.
    pub tree: Vec<TreeEntry>,
    /// What crank prints for the tree, without color.
    pub output: String,
    /// Every file crank creates or changes (`.crank.manifest` aside), as
    /// `(path, contents)`, in the order the example lists them.
    #[serde(skip)]
    pub generated: Vec<(String, String)>,
}

/// Parses example `id`'s Markdown `source`:
///
/// - a `# Title` first line;
/// - the explanation, up to the first `## ` heading;
/// - one section per file, headed by its path in backticks
///   (``## `.crank` ``, ``## `templates/page.html.tera` ``), holding its
///   contents;
/// - a `## Output` section holding what crank prints;
/// - one ``## Generated `path` `` section per file crank creates or
///   changes, holding what it writes there.
///
/// Every section holds exactly one fenced code block and nothing else; a
/// file's contents are the block's lines, each ending in a newline.
///
/// # Errors
/// Returns a description of the first thing that doesn't follow that
/// format.
pub fn parse(id: &str, source: &str) -> Result<Example, String> {
    let mut lines = source.lines();
    let title = lines
        .next()
        .and_then(|line| line.strip_prefix("# "))
        .ok_or("the first line isn't a `# ` title")?
        .trim()
        .to_string();

    let mut explanation = Vec::new();
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    for line in lines {
        if let Some(heading) = line.strip_prefix("## ") {
            sections.push((heading.trim().to_string(), Vec::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push(line);
        } else {
            explanation.push(line);
        }
    }

    let mut tree = Vec::new();
    let mut generated = Vec::new();
    let mut output = None;
    for (heading, body) in &sections {
        let block = code_block(body).map_err(|err| format!("`## {heading}`: {err}"))?;
        let contents = || -> String { block.iter().flat_map(|line| [*line, "\n"]).collect() };
        if heading == "Output" {
            output = Some(block.join("\n"));
        } else if let Some(path) = heading
            .strip_prefix("Generated `")
            .and_then(|rest| rest.strip_suffix('`'))
        {
            generated.push((path.to_string(), contents()));
        } else if let Some(path) = heading
            .strip_prefix('`')
            .and_then(|rest| rest.strip_suffix('`'))
        {
            tree.push(TreeEntry::file(path, &contents()));
        } else {
            return Err(format!(
                "unknown section `## {heading}` (expected a `path`, Output or Generated `path`)"
            ));
        }
    }

    if tree.is_empty() {
        return Err("no file sections".to_string());
    }
    Ok(Example {
        id: id.to_string(),
        title,
        explanation: explanation.join("\n").trim().to_string(),
        tree,
        output: output.ok_or("no `## Output` section")?,
        generated,
    })
}

/// The lines inside the one fenced code block that makes up `body`,
/// which may otherwise hold only blank lines. The block closes with the
/// same run of backticks that opened it, so a longer fence can hold a
/// shorter one.
fn code_block<'a>(body: &[&'a str]) -> Result<Vec<&'a str>, String> {
    let mut rest = body.iter().skip_while(|line| line.trim().is_empty());
    let fence = rest
        .next()
        .filter(|line| line.starts_with("```"))
        .map(|line| &line[..line.len() - line.trim_start_matches('`').len()])
        .ok_or("expected a fenced code block")?;
    let mut block = Vec::new();
    for line in rest.by_ref() {
        if *line == fence {
            return if rest.all(|line| line.trim().is_empty()) {
                Ok(block)
            } else {
                Err("unexpected text after the code block".to_string())
            };
        }
        block.push(*line);
    }
    Err("unterminated code block".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FENCE: &str = "```";

    fn source(sections: &str) -> String {
        format!("# A title\n\nSome *explanation*.\n\nMore.\n\n{sections}")
    }

    #[test]
    fn a_complete_example_parses() {
        let source = source(&format!(
            "## `.crank`\n\n{FENCE}yaml\ntaskGroups: []\n{FENCE}\n\n\
             ## `templates/a.tera`\n\n{FENCE}text\n{{{{ name }}}}\n\n{FENCE}\n\n\
             ## Output\n\n{FENCE}text\nline 1\n  line 2\n{FENCE}\n\n\
             ## Generated `a.txt`\n\n{FENCE}text\nwidget\n{FENCE}\n"
        ));
        assert_eq!(
            parse("01-x", &source),
            Ok(Example {
                id: "01-x".to_string(),
                title: "A title".to_string(),
                explanation: "Some *explanation*.\n\nMore.".to_string(),
                tree: vec![
                    TreeEntry::file(".crank", "taskGroups: []\n"),
                    TreeEntry::file("templates/a.tera", "{{ name }}\n\n"),
                ],
                output: "line 1\n  line 2".to_string(),
                generated: vec![("a.txt".to_string(), "widget\n".to_string())],
            })
        );
    }

    #[test]
    fn a_longer_fence_can_hold_a_shorter_one() {
        let source = source(&format!(
            "## `a.md`\n\n````markdown\n{FENCE}\ncode\n{FENCE}\n````\n\n\
             ## Output\n\n{FENCE}text\n{FENCE}\n"
        ));
        assert_eq!(
            parse("x", &source).map(|example| example.tree),
            Ok(vec![TreeEntry::file("a.md", "```\ncode\n```\n")])
        );
    }

    #[test]
    fn a_missing_title_is_an_error() {
        assert_eq!(
            parse("x", "Title\n"),
            Err("the first line isn't a `# ` title".to_string())
        );
    }

    #[test]
    fn missing_sections_are_errors() {
        let output = format!("## Output\n\n{FENCE}text\nok\n{FENCE}\n");
        assert_eq!(
            parse("x", &source(&output)),
            Err("no file sections".to_string())
        );
        let files = format!("## `.crank`\n\n{FENCE}text\n{FENCE}\n");
        assert_eq!(
            parse("x", &source(&files)),
            Err("no `## Output` section".to_string())
        );
    }

    #[test]
    fn an_unknown_section_is_an_error() {
        let source = source(&format!("## Notes\n\n{FENCE}text\n{FENCE}\n"));
        assert_eq!(
            parse("x", &source),
            Err(
                "unknown section `## Notes` (expected a `path`, Output or Generated `path`)"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_section_must_be_one_code_block() {
        let no_block = source("## `a`\n\nprose\n");
        assert_eq!(
            parse("x", &no_block),
            Err("`## `a``: expected a fenced code block".to_string())
        );
        let unterminated = source(&format!("## `a`\n\n{FENCE}text\na\n"));
        assert_eq!(
            parse("x", &unterminated),
            Err("`## `a``: unterminated code block".to_string())
        );
        let trailing = source(&format!("## `a`\n\n{FENCE}text\na\n{FENCE}\nprose\n"));
        assert_eq!(
            parse("x", &trailing),
            Err("`## `a``: unexpected text after the code block".to_string())
        );
    }
}
