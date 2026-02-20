use std::fs;
use zed::LanguageServerId;
use zed_extension_api::{self as zed, Result};

struct NeoCMakeExt {
    cached_binary_path: Option<String>,
}

impl NeoCMakeExt {
    fn find_cached_binary_on_drive(&self, exe_suffix: &str) -> Option<String> {
        fs::read_dir(".")
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let dir_name = entry.file_name();
                let dir_name = dir_name.to_string_lossy();

                if !dir_name.starts_with("neocmakelsp-") {
                    return None;
                }

                // Extract version part
                let version_str = dir_name.strip_prefix("neocmakelsp-v")?;

                // Parse version numbers
                let mut parts = version_str.split('.');
                let major = parts.next()?.parse::<u32>().ok()?;
                let minor = parts.next()?.parse::<u32>().ok()?;
                let patch = parts.next()?.parse::<u32>().ok()?;

                // Ensure no extra parts
                if parts.next().is_some() {
                    return None;
                }

                let candidate = format!("{}/neocmakelsp{}", dir_name, exe_suffix);

                fs::metadata(&candidate)
                    .ok()
                    .filter(|m| m.is_file())
                    .map(|_| ((major, minor, patch), candidate))
            })
            .max_by_key(|(version, _)| *version)
            .map(|(_, path)| path)
    }

    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        if let Some(path) = worktree.which("neocmakelsp") {
            return Ok(path);
        }

        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).map_or(false, |stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        let (platform, arch) = zed::current_platform();
        let exe_suffix = match platform {
            zed::Os::Windows => ".exe",
            _ => "",
        };

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            "Decodetalkers/neocmakelsp",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        );

        let release = match release {
            Ok(release) => release,
            Err(e) => {
                eprintln!("neocmakelsp: GitHub unreachable ({e}), looking for cached binary");
                return self
                    .find_cached_binary_on_drive(exe_suffix)
                    .ok_or_else(|| format!("GitHub unreachable and no cached binary found: {e}"));
            }
        };

        let asset_name = match (platform, arch) {
            (zed::Os::Mac, _) => "neocmakelsp-universal-apple-darwin.tar.gz",
            (zed::Os::Windows, zed::Architecture::Aarch64) => {
                "neocmakelsp-aarch64-pc-windows-msvc.zip"
            }
            (zed::Os::Windows, zed::Architecture::X8664) => {
                "neocmakelsp-x86_64-pc-windows-msvc.zip"
            }
            (zed::Os::Linux, zed::Architecture::Aarch64) => {
                "neocmakelsp-aarch64-unknown-linux-gnu.tar.gz"
            }
            (zed::Os::Linux, zed::Architecture::X8664) => {
                "neocmakelsp-x86_64-unknown-linux-gnu.tar.gz"
            }
            _ => {
                return Err(format!(
                    "Unsupported platform-arch combination: {:?} {:?}",
                    platform, arch
                ))
            }
        };
        let asset_type = match platform {
            zed::Os::Mac | zed::Os::Linux => zed::DownloadedFileType::GzipTar,
            zed::Os::Windows => zed::DownloadedFileType::Zip,
        };

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

        let version_dir = format!("neocmakelsp-{}", release.version);
        let binary_path = format!("{version_dir}/neocmakelsp{exe_suffix}"); // Line 65 moment

        if !fs::metadata(&binary_path).map_or(false, |stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(&asset.download_url, &version_dir, asset_type)
                .map_err(|e| format!("failed to download file: {e}"))?;

            zed::make_file_executable(&binary_path)?;

            // Remove old versions
            let entries =
                fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                if entry.file_name().to_str() != Some(&version_dir) {
                    fs::remove_dir_all(entry.path()).ok();
                }
            }
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl zed::Extension for NeoCMakeExt {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: self.language_server_binary_path(language_server_id, worktree)?,
            args: vec![String::from("stdio")],
            env: Default::default(),
        })
    }
}

zed::register_extension!(NeoCMakeExt);
