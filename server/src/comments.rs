//! Finds tagged comment lines, e.g. `// ! careful`, `// ? why`, `// * note`, `// TODO: later`.

use tree_sitter::{Node, Tree};

use crate::config::Tag;
use crate::languages::Language;

/// One tagged comment line. Columns are byte offsets into the line.
#[derive(Debug)]
pub struct TaggedLine<'t> {
    pub row: usize,
    pub start: usize,
    pub end: usize,
    pub tag: &'t Tag,
}

/// Tags each comment line by the prefix that follows the comment opener. Block comments are
/// tagged line by line, so one `/* */` can hold several tags.
pub fn tagged_lines<'t>(
    tree: &Tree,
    source: &str,
    language: &Language,
    tags: &'t [Tag],
) -> Vec<TaggedLine<'t>> {
    let mut found = Vec::new();
    collect(tree.root_node(), source, language, tags, &mut found);
    found
}

fn collect<'t>(
    node: Node,
    source: &str,
    language: &Language,
    tags: &'t [Tag],
    found: &mut Vec<TaggedLine<'t>>,
) {
    if language.comments.contains(&node.kind()) {
        tag_comment(node, source, language, tags, found);
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect(child, source, language, tags, found);
    }
}

fn tag_comment<'t>(
    node: Node,
    source: &str,
    language: &Language,
    tags: &'t [Tag],
    found: &mut Vec<TaggedLine<'t>>,
) {
    let start = node.start_position();
    for (index, line) in source[node.byte_range()].split('\n').enumerate() {
        let line = line.trim_end();
        let indent = line.len() - line.trim_start().len();
        let content = line.trim_start();
        let body = if index == 0 {
            let marker = language
                .comment_markers
                .iter()
                .find(|marker| content.starts_with(**marker));
            marker.map_or(content, |marker| &content[marker.len()..])
        } else {
            content
                .strip_prefix('*')
                .filter(|rest| !rest.starts_with('/'))
                .unwrap_or(content)
        };
        let Some(tag) = tag_of(body.trim_start(), tags) else {
            continue;
        };
        let column = if index == 0 { start.column } else { 0 };
        found.push(TaggedLine {
            row: start.row + index,
            start: column + indent,
            end: column + line.len(),
            tag,
        });
    }
}

/// The tag with the longest prefix that `text` starts with. Word prefixes like `TODO` must end
/// at a word boundary, so `TODOS` is not a tag. A closing `*/` is never a tag.
fn tag_of<'t>(text: &str, tags: &'t [Tag]) -> Option<&'t Tag> {
    if text.starts_with("*/") {
        return None;
    }
    tags.iter()
        .filter(|tag| {
            let is_word = tag.prefix.ends_with(|c: char| c.is_alphanumeric());
            !tag.prefix.is_empty()
                && text.strip_prefix(tag.prefix.as_str()).is_some_and(|rest| {
                    !(is_word && rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
                })
        })
        .max_by_key(|tag| tag.prefix.len())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::config::Config;
    use crate::languages;

    /// `(row, modifier)` per tagged line.
    fn tagged(file: &str, source: &str, tags: &[Tag]) -> Vec<(usize, String)> {
        let language = languages::for_path(Path::new(file)).unwrap();
        let tree = language.parse(source).unwrap();
        tagged_lines(&tree, source, &language, tags)
            .into_iter()
            .map(|line| (line.row, line.tag.modifier.clone()))
            .collect()
    }

    fn with_defaults(file: &str, source: &str) -> Vec<(usize, String)> {
        tagged(file, source, &Config::default().tags)
    }

    fn rows(expected: &[(usize, &str)]) -> Vec<(usize, String)> {
        expected
            .iter()
            .map(|(row, modifier)| (*row, modifier.to_string()))
            .collect()
    }

    #[test]
    fn tags_line_and_block_comments() {
        let source = "// ! alert
// ? question
/**
 * * highlight
 * plain
 */
// TODO: later
// TODOS is not a tag
let x = 1; // plain";
        assert_eq!(
            with_defaults("a.ts", source),
            rows(&[(0, "alert"), (1, "question"), (3, "highlight"), (6, "todo")])
        );
    }

    #[test]
    fn spans_the_comment_text_only() {
        let source = "let x = 1; // ! alert   ";
        let language = languages::for_path(Path::new("a.ts")).unwrap();
        let tree = language.parse(source).unwrap();
        let config = Config::default();
        let [line] = tagged_lines(&tree, source, &language, &config.tags)
            .try_into()
            .unwrap();
        assert_eq!((line.row, line.start, line.end), (0, 11, 21));
    }

    #[test]
    fn ignores_rust_doc_markers() {
        assert_eq!(
            with_defaults("a.rs", "//! inner doc\n/// outer doc\n// ! alert\n"),
            rows(&[(2, "alert")])
        );
    }

    #[test]
    fn prefers_the_longest_custom_prefix() {
        let tag = |prefix: &str, modifier: &str| Tag {
            prefix: prefix.into(),
            modifier: modifier.into(),
        };
        let tags = [tag("!", "alert"), tag("!!", "danger"), tag("HACK", "hack")];
        assert_eq!(
            tagged("a.ts", "// !! no\n// ! yes\n// HACK: x\n// HACKY\n", &tags),
            rows(&[(0, "danger"), (1, "alert"), (2, "hack")])
        );
    }
}
