//! User settings, read once at start-up from Zed's
//! `lsp.better-highlights-lsp.initialization_options`. Missing fields keep their defaults.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Functions scoring above this get a warning. SonarSource's default is 15.
    pub complexity_limit: u32,
    /// Functions scoring below this get no hint, to keep simple code quiet.
    pub hint_threshold: u32,
    /// Comment tags. Setting this replaces the defaults rather than adding to them.
    pub tags: Vec<Tag>,
}

/// A comment starting with `prefix` (after the comment opener) becomes a `comment` semantic
/// token with the `modifier` modifier, which `semantic_token_rules` can then style.
#[derive(Debug, Deserialize)]
pub struct Tag {
    pub prefix: String,
    pub modifier: String,
}

impl Default for Config {
    fn default() -> Self {
        let tag = |prefix: &str, modifier: &str| Tag {
            prefix: prefix.into(),
            modifier: modifier.into(),
        };
        Config {
            complexity_limit: 15,
            hint_threshold: 5,
            tags: vec![
                tag("!", "alert"),
                tag("?", "question"),
                tag("*", "highlight"),
                tag("TODO", "todo"),
                tag("FIXME", "fixme"),
                tag("BUG", "fixme"),
                tag("HACK", "hack"),
                tag("XXX", "hack"),
                tag("NOTE", "note"),
                tag("INFO", "note"),
                tag("DEPRECATED", "deprecated"),
            ],
        }
    }
}

impl Config {
    /// Parses `initialization_options`, falling back to defaults (with a note on stderr, which
    /// Zed shows in the server log) when they don't fit.
    pub fn from_options(options: Option<serde_json::Value>) -> Config {
        let Some(options) = options else {
            return Config::default();
        };
        serde_json::from_value(options).unwrap_or_else(|error| {
            eprintln!("better-highlights: ignoring invalid initialization_options: {error}");
            Config::default()
        })
    }

    /// Distinct modifier names, in first-seen order. Bit `i` of a token's modifier set is entry
    /// `i`; LSP bitsets are 32 bits, so tags past the 32nd modifier are dropped.
    pub fn modifiers(&self) -> Vec<&str> {
        let mut modifiers: Vec<&str> = Vec::new();
        for tag in &self.tags {
            if !modifiers.contains(&tag.modifier.as_str()) && modifiers.len() < 32 {
                modifiers.push(&tag.modifier);
            }
        }
        modifiers
    }
}
