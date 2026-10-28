# Better Highlights for Zed

Cognitive complexity hints for functions, and colours for tagged comments:

```ts
// ! alert                  (bold red)
// ? question               (italic blue)
// * highlight              (green)
// TODO: later              (orange)
// FIXME: broken            (bold red; also BUG)
// HACK: shortcut           (purple; also XXX)
// NOTE: context            (italic teal; also INFO)
// DEPRECATED: use y        (grey, struck through)
```

Supports Rust, TypeScript, TSX, JavaScript, C#, Python, Go and Java. Java needs Zed's Java extension.

## Layout

- `server/`: the language server (`better-highlights-lsp`). It parses with tree-sitter and serves inlay hints and semantic tokens.
- `src/lib.rs`, `extension.toml`: the Zed extension. It starts the server from `PATH`, or downloads it from the latest GitHub release.
- `.github/workflows/release.yml`: builds the server for Windows, macOS and Linux when a `v*` tag is pushed.

## Install

Better Highlights isn't in Zed's extension registry yet. To install it from GitHub:

1. `git clone https://github.com/MalasataXD/better-highlights`
2. In Zed, run `zed: install dev extension` and pick the cloned folder. Zed builds the extension, which needs Rust installed via [rustup](https://rustup.rs). The extension downloads the server on first use.
3. Add the settings below.

To work on the server, run `cargo install --path server` instead. A `better-highlights-lsp` on `PATH` takes precedence over the download. Restart Zed after adding it to `PATH`.

## Settings

Zed has inlay hints and semantic tokens off by default, and an extension can't ship comment colours. Add the following to your user `settings.json`. `global_lsp_settings` is rejected in project settings.

```json
{
  "inlay_hints": { "enabled": true },
  "semantic_tokens": "combined",
  "global_lsp_settings": {
    "semantic_token_rules": [
      { "token_type": "comment", "token_modifiers": ["alert"], "foreground_color": "#FF453A", "font_weight": "bold" },
      { "token_type": "comment", "token_modifiers": ["question"], "foreground_color": "#3B9EFF", "font_style": "italic" },
      { "token_type": "comment", "token_modifiers": ["highlight"], "foreground_color": "#98C379" },
      { "token_type": "comment", "token_modifiers": ["todo"], "foreground_color": "#FF8C00" },
      { "token_type": "comment", "token_modifiers": ["fixme"], "foreground_color": "#FF453A", "font_weight": "bold" },
      { "token_type": "comment", "token_modifiers": ["hack"], "foreground_color": "#C678DD" },
      { "token_type": "comment", "token_modifiers": ["note"], "foreground_color": "#56B6C2", "font_style": "italic" },
      { "token_type": "comment", "token_modifiers": ["deprecated"], "foreground_color": "#7F848E", "strikethrough": true }
    ]
  }
}
```

## Complexity

The score follows [SonarSource's cognitive complexity](https://www.sonarsource.com/docs/CognitiveComplexity.pdf), with two exceptions: labelled jumps and recursion are not counted. Named functions get their own hint. A lambda adds nesting to its enclosing function, and gets its own hint only at the top level.

Hints read `complexity 14/15`: the score, then the limit. Functions scoring below 5 get no hint. Hover a hint to see the costliest blocks. A function above the limit also gets:

- a warning on its name, with the same explanation;
- a `+N complexity (M from nesting)` hint on each top-level block inside it, so you can see where the cost sits without hovering.

## Options

Set these under `lsp.better-highlights-lsp.initialization_options` in your Zed settings, then run `editor: restart language server`. These are the defaults:

```json
{
  "lsp": {
    "better-highlights-lsp": {
      "initialization_options": {
        "complexity_limit": 15,
        "hint_threshold": 5,
        "tags": [
          { "prefix": "!", "modifier": "alert" },
          { "prefix": "?", "modifier": "question" },
          { "prefix": "*", "modifier": "highlight" },
          { "prefix": "TODO", "modifier": "todo" },
          { "prefix": "FIXME", "modifier": "fixme" },
          { "prefix": "BUG", "modifier": "fixme" },
          { "prefix": "HACK", "modifier": "hack" },
          { "prefix": "XXX", "modifier": "hack" },
          { "prefix": "NOTE", "modifier": "note" },
          { "prefix": "INFO", "modifier": "note" },
          { "prefix": "DEPRECATED", "modifier": "deprecated" }
        ]
      }
    }
  }
}
```

- `complexity_limit`: functions scoring above this get a warning.
- `hint_threshold`: functions scoring below this get no hint. Set it to `0` to show every hint.
- `tags`: setting this replaces the default list. The longest matching prefix wins. A word prefix such as `TODO` must be followed by a non-word character, so `TODOS` doesn't match. Each `modifier` needs a matching entry in `semantic_token_rules` to get a style (`foreground_color`, `background_color`, `font_weight: "bold"`, `font_style: "italic"`, `underline`, `strikethrough`), for example `{ "token_type": "comment", "token_modifiers": ["hack"], "foreground_color": "#C678DD" }`.

## Credits

The idea comes from [Better Highlights](https://plugins.jetbrains.com/plugin/12895-better-highlights) for JetBrains IDEs, by [Igor Zarvanskyi](https://clutcher.github.io/). This is an independent reimplementation for Zed, not affiliated with that plugin.
