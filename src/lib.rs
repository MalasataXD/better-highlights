//! Zed extension that launches `better-highlights-lsp` and hands it the user's
//! `lsp.better-highlights-lsp.initialization_options`. The server comes from PATH when present
//! (local builds), otherwise from the latest GitHub release.

use std::fs;

use zed_extension_api::{
    self as zed, Architecture, DownloadedFileType, GithubReleaseOptions, LanguageServerId,
    LanguageServerInstallationStatus, Os, Result, serde_json, settings::LspSettings,
};

const SERVER: &str = "better-highlights-lsp";
const REPOSITORY: &str = "MalasataXD/better-highlights";

struct BetterHighlights {
    /// Path of the downloaded server, once found or fetched this session.
    downloaded: Option<String>,
}

impl zed::Extension for BetterHighlights {
    fn new() -> Self {
        BetterHighlights { downloaded: None }
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let command = match worktree.which(SERVER) {
            Some(path) => path,
            None => self.download(id)?,
        };
        Ok(zed::Command {
            command,
            args: Vec::new(),
            env: Vec::new(),
        })
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

impl BetterHighlights {
    /// Downloads the latest release's server for this platform into `better-highlights-lsp-<version>/`,
    /// unless it is already there, and removes older versions.
    fn download(&mut self, id: &LanguageServerId) -> Result<String> {
        if let Some(path) = &self.downloaded
            && fs::metadata(path).is_ok()
        {
            return Ok(path.clone());
        }

        zed::set_language_server_installation_status(
            id,
            &LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let release = zed::latest_github_release(
            REPOSITORY,
            GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (os, architecture) = zed::current_platform();
        let target = match (os, architecture) {
            (Os::Windows, Architecture::X8664) => "x86_64-pc-windows-msvc",
            (Os::Mac, Architecture::Aarch64) => "aarch64-apple-darwin",
            (Os::Mac, Architecture::X8664) => "x86_64-apple-darwin",
            (Os::Linux, Architecture::X8664) => "x86_64-unknown-linux-gnu",
            (Os::Linux, Architecture::Aarch64) => "aarch64-unknown-linux-gnu",
            _ => {
                return Err(format!(
                    "{SERVER} has no build for {os:?} on {architecture:?}"
                ));
            }
        };
        let (archive, file_type, executable) = match os {
            Os::Windows => ("zip", DownloadedFileType::Zip, format!("{SERVER}.exe")),
            _ => ("tar.gz", DownloadedFileType::GzipTar, SERVER.to_string()),
        };
        let asset_name = format!("{SERVER}-{target}.{archive}");
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("release {} has no asset {asset_name}", release.version))?;

        let directory = format!("{SERVER}-{}", release.version);
        let path = format!("{directory}/{executable}");
        if fs::metadata(&path).is_err() {
            zed::set_language_server_installation_status(
                id,
                &LanguageServerInstallationStatus::Downloading,
            );
            zed::download_file(&asset.download_url, &directory, file_type)
                .map_err(|error| format!("failed to download {asset_name}: {error}"))?;
            zed::make_file_executable(&path)?;
            remove_other_versions(&directory);
        }

        self.downloaded = Some(path.clone());
        Ok(path)
    }
}

/// Deletes downloaded server versions other than `keep` from the extension's work directory.
fn remove_other_versions(keep: &str) {
    let Ok(entries) = fs::read_dir(".") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(SERVER) && name != keep {
            fs::remove_dir_all(entry.path()).ok();
        }
    }
}

zed::register_extension!(BetterHighlights);
