// SPDX-FileCopyrightText: The crank contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pure logic for `crank --check`: classifying what a normal run would do
//! to a single output path, without touching the filesystem.

/// How writing freshly rendered content to a path would affect it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// No file exists at the path yet, so writing would create one.
    Created,
    /// A file exists there with different content, which writing would
    /// replace.
    Modified,
    /// A file exists there with exactly the content that would be
    /// written.
    Unchanged,
}

/// Classifies what writing `rendered` to a path would do, given that
/// path's current content (`None` if no file exists there yet).
#[must_use]
pub fn classify(existing: Option<&str>, rendered: &str) -> ChangeKind {
    match existing {
        None => ChangeKind::Created,
        Some(existing) if existing == rendered => ChangeKind::Unchanged,
        Some(_) => ChangeKind::Modified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_existing_file_is_created() {
        assert_eq!(classify(None, "anything"), ChangeKind::Created);
    }

    #[test]
    fn identical_content_is_unchanged() {
        assert_eq!(classify(Some("same"), "same"), ChangeKind::Unchanged);
    }

    #[test]
    fn differing_content_is_modified() {
        assert_eq!(classify(Some("old"), "new"), ChangeKind::Modified);
    }

    #[test]
    fn empty_existing_file_matching_empty_rendered_is_unchanged() {
        assert_eq!(classify(Some(""), ""), ChangeKind::Unchanged);
    }

    #[test]
    fn no_existing_file_with_empty_rendered_content_is_still_created() {
        assert_eq!(classify(None, ""), ChangeKind::Created);
    }
}
