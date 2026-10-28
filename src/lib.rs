//! Zed extension that launches `better-highlights-lsp` from PATH and hands it the user's
//! `lsp.better-highlights-lsp.initialization_options`.

use zed_extension_api::{self as zed, LanguageServerId, Result, serde_json, settings::LspSettings};

const SERVER: &str = "better-highlights-lsp";

struct BetterHighlights;

impl zed::Extension for BetterHighlights {
    fn new() -> Self {
        BetterHighlights
    }

    fn language_server_command(&mut self, _: &LanguageServerId, worktree: &zed::Worktree) -> Result<zed::Command> {
        let command = worktree
            .which(SERVER)
            .ok_or_else(|| format!("`{SERVER}` is not on PATH. Install it with `cargo install --path server`."))?;
        Ok(zed::Command { command, args: Vec::new(), env: Vec::new() })
    }

    fn language_server_initialization_options(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        // No settings for this server is normal, not an error: the server then uses its defaults.
        let settings = LspSettings::for_worktree(id.as_ref(), worktree).ok();
        Ok(settings.and_then(|settings| settings.initialization_options))
    }
}

zed::register_extension!(BetterHighlights);
