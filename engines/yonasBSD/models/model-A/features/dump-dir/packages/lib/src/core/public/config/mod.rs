use std::path::{Path, PathBuf};

use config::{Config as ConfigRs, File, FileFormat};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;

use crate::errors::{ConfigLoadSnafu, ConfigNotFoundSnafu, DumpResult};

/// The resolved, merged configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    /// File extensions to skip (without leading dot), e.g. ["snap", "lock"]
    pub skip_extensions: Vec<String>,

    /// Filename patterns to skip (regex), e.g. [".*\.test\.rs$"]
    pub skip_patterns: Vec<String>,

    /// Exact filenames to skip (case-insensitive), e.g. ["license", "makefile"]
    pub skip_filenames: Vec<String>,

    /// Path component names that cause a file to be skipped if any component matches.
    /// e.g. [".github", ".git", "node_modules"]
    pub skip_path_components: Vec<String>,

    /// Glob patterns matched against the full file path, e.g. ["**/target/**", "**/*.min.js"]
    pub skip_globs: Vec<String>,

    /// If true, skip files detected as binary by MIME sniffing
    pub skip_binary: bool,

    /// If true, skip hidden files and directories (any component starting with '.')
    pub skip_hidden: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            skip_extensions: vec![
                "snap".into(),
                "lock".into(),
                "new".into(),
                "gitignore".into(),
                "orig".into(),
                "bak".into(),
                "swp".into(),
            ],
            skip_patterns: vec![r".*test.*\.rs$".into()],
            skip_filenames: vec![
                "license".into(),
                "readme".into(),
                "changelog".into(),
                "makefile".into(),
                "dockerfile".into(),
            ],
            skip_path_components: vec![
                ".github".into(),
                ".git".into(),
                "node_modules".into(),
                ".direnv".into(),
            ],
            skip_globs: vec![],
            skip_binary: true,
            skip_hidden: true,
        }
    }
}

/// Load config by layering:
///   1. Built-in defaults (via `AppConfig::default()`)
///   2. Global config:  ~/.config/dump-dir/config.toml  (if it exists)
///   3. Local config:   ./dump.toml  (or --config path)  (if it exists)
///
/// Later layers override earlier ones. Arrays are replaced, not merged.
pub fn load(local_override: Option<&Path>) -> DumpResult<AppConfig> {
    let mut builder = ConfigRs::builder();

    // --- Layer 1: Global config ---
    if let Some(home) = home_dir() {
        let global: PathBuf = home.join(".config").join("dump-dir").join("config.toml");
        if global.exists() {
            builder = builder.add_source(
                File::from(global.as_path())
                    .format(FileFormat::Toml)
                    .required(false),
            );
        }
    }

    // --- Layer 2: Local config (./dump.toml or --config path) ---
    let local_path: PathBuf = local_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("dump.toml"));

    if local_path.exists() {
        builder = builder.add_source(
            File::from(local_path.as_path())
                .format(FileFormat::Toml)
                .required(false),
        );
    } else if local_override.is_some() {
        // User explicitly passed --config but the file doesn't exist — typed error
        return ConfigNotFoundSnafu {
            path: local_path.display().to_string(),
        }
        .fail();
    }

    let raw = builder.build().context(ConfigLoadSnafu)?;

    let cfg: AppConfig = raw.try_deserialize().context(ConfigLoadSnafu)?;

    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn write_toml(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        path
    }

    // ── Defaults ───────────────────────────────────────────────────────────

    #[test]
    fn default_has_expected_extensions() {
        let cfg = AppConfig::default();
        assert!(cfg.skip_extensions.contains(&"lock".to_string()));
        assert!(cfg.skip_extensions.contains(&"snap".to_string()));
    }

    #[test]
    fn default_skip_binary_is_true() {
        assert!(AppConfig::default().skip_binary);
    }

    #[test]
    fn default_skip_hidden_is_true() {
        assert!(AppConfig::default().skip_hidden);
    }

    #[test]
    fn default_skip_globs_is_empty() {
        assert!(AppConfig::default().skip_globs.is_empty());
    }

    // ── Local config loading ───────────────────────────────────────────────

    #[test]
    fn loads_local_config_overriding_extensions() {
        let dir = TempDir::new().unwrap();
        write_toml(&dir, "dump.toml", r#"skip_extensions = ["foo", "bar"]"#);
        let cfg = load(Some(&dir.path().join("dump.toml"))).unwrap();
        assert_eq!(cfg.skip_extensions, vec!["foo", "bar"]);
    }

    #[test]
    fn loads_local_config_skip_binary_false() {
        let dir = TempDir::new().unwrap();
        write_toml(&dir, "dump.toml", "skip_binary = false");
        let cfg = load(Some(&dir.path().join("dump.toml"))).unwrap();
        assert!(!cfg.skip_binary);
    }

    #[test]
    fn loads_local_config_with_globs() {
        let dir = TempDir::new().unwrap();
        write_toml(
            &dir,
            "dump.toml",
            r#"skip_globs = ["**/target/**", "**/*.min.js"]"#,
        );
        let cfg = load(Some(&dir.path().join("dump.toml"))).unwrap();
        assert_eq!(cfg.skip_globs.len(), 2);
        assert!(cfg.skip_globs.contains(&"**/target/**".to_string()));
    }

    #[test]
    fn missing_explicit_config_returns_error() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("nope.toml");
        let result = load(Some(&nonexistent));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Config file not found"));
        // Verify it's the typed variant we expect
        assert!(matches!(
            err,
            crate::errors::DumpError::ConfigNotFound { .. }
        ));
    }

    #[test]
    fn missing_default_local_config_uses_defaults() {
        let cfg = load(None);
        drop(cfg);
    }

    #[test]
    fn invalid_toml_returns_error() {
        let dir = TempDir::new().unwrap();
        write_toml(&dir, "bad.toml", "this is not [ valid toml !!!");
        let result = load(Some(&dir.path().join("bad.toml")));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::errors::DumpError::ConfigLoad { .. }
        ));
    }

    #[test]
    fn partial_config_fills_missing_fields_from_defaults() {
        let dir = TempDir::new().unwrap();
        write_toml(&dir, "dump.toml", "skip_binary = false");
        let cfg = load(Some(&dir.path().join("dump.toml"))).unwrap();
        assert!(!cfg.skip_binary);
        assert!(cfg.skip_hidden);
        assert!(!cfg.skip_extensions.is_empty());
    }
}
