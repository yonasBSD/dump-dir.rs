use std::path::Path;

use anyhow::Result;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::Regex;

use crate::config::AppConfig;

pub struct Filter {
    skip_extensions: Vec<String>,
    skip_filenames: Vec<String>,
    skip_path_components: Vec<String>,
    skip_patterns: Vec<Regex>,
    skip_globs: GlobSet,
    skip_binary: bool,
    skip_hidden: bool,
}

impl Filter {
    pub fn new(cfg: &AppConfig) -> Result<Self> {
        let skip_patterns = cfg
            .skip_patterns
            .iter()
            .map(|p| {
                Regex::new(&format!("(?i){p}"))
                    .map_err(|e| anyhow::anyhow!("Invalid regex pattern '{p}': {e}"))
            })
            .collect::<Result<Vec<_>>>()?;

        let mut glob_builder = GlobSetBuilder::new();
        for pattern in &cfg.skip_globs {
            let glob = GlobBuilder::new(pattern)
                .case_insensitive(true)
                .literal_separator(true) // ** crosses dirs, * does not — correct glob semantics
                .build()
                .map_err(|e| anyhow::anyhow!("Invalid glob pattern '{pattern}': {e}"))?;
            glob_builder.add(glob);
        }
        let skip_globs = glob_builder
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build glob set: {e}"))?;

        Ok(Self {
            skip_extensions: cfg
                .skip_extensions
                .iter()
                .map(|s| s.to_lowercase())
                .collect(),
            skip_filenames: cfg
                .skip_filenames
                .iter()
                .map(|s| s.to_lowercase())
                .collect(),
            skip_path_components: cfg
                .skip_path_components
                .iter()
                .map(|s| s.to_lowercase())
                .collect(),
            skip_patterns,
            skip_globs,
            skip_binary: cfg.skip_binary,
            skip_hidden: cfg.skip_hidden,
        })
    }

    /// Returns `true` if an entire directory should be pruned from the walk.
    /// Faster than waiting to reject every file inside it individually.
    pub fn should_skip_dir(&self, path: &Path) -> bool {
        // Check the directory name itself (the last component)
        if let Some(name) = path.file_name() {
            let name_lower = name.to_string_lossy().to_lowercase();

            if self.skip_hidden && name_lower.starts_with('.') {
                return true;
            }
            if self.skip_path_components.contains(&name_lower) {
                return true;
            }
        }

        // Check glob patterns against the directory path.
        // For a pattern like **/target/**, we need to match both:
        //   - the path itself (e.g. /proj/target)
        //   - a synthetic child path (e.g. /proj/target/_) so trailing /** is satisfied
        // We try multiple forms and also strip the cwd prefix for relative matching.
        let synthetic = path.join("_");
        for candidate in [path, synthetic.as_path()] {
            if self.skip_globs.is_match(candidate) {
                return true;
            }
            if let Ok(rel) = candidate.strip_prefix(std::env::current_dir().unwrap_or_default()) {
                if self.skip_globs.is_match(rel) {
                    return true;
                }
            }
        }

        false
    }

    /// Returns `true` if the file should be skipped.
    pub fn should_skip(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        // --- Check each path component for blocked names ---
        for component in path.components() {
            let c = component.as_os_str().to_string_lossy().to_lowercase();

            // Skip blocked path components (e.g. node_modules, .github)
            if self.skip_path_components.contains(&c) {
                return true;
            }
        }

        // --- Check hidden components ---
        // For relative paths (e.g. in unit tests or direct calls), we must
        // check every component since there's no walker to prune ancestors.
        // For absolute paths, the walker's filter_entry already pruned hidden
        // directories, so we only need to check the filename itself to avoid
        // false positives from dotted segments in the absolute prefix (e.g.
        // a temp dir like /tmp/.tmpXYZ/main.rs).
        if self.skip_hidden {
            if path.is_absolute() {
                if let Some(name) = path.file_name() {
                    if name.to_string_lossy().starts_with('.') {
                        return true;
                    }
                }
            } else {
                for component in path.components() {
                    let c = component.as_os_str().to_string_lossy();
                    if c.starts_with('.') && c != "." && c != ".." {
                        return true;
                    }
                }
            }
        }

        // --- Check extension ---
        if let Some(ext) = path.extension() {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            if self.skip_extensions.contains(&ext_lower) {
                return true;
            }
        }

        // --- Check filename (stem only, case-insensitive) ---
        if let Some(name) = path.file_stem() {
            let name_lower = name.to_string_lossy().to_lowercase();
            if self.skip_filenames.contains(&name_lower) {
                return true;
            }
        }
        // Also check the full filename (e.g. "Makefile" has no extension)
        if let Some(name) = path.file_name() {
            let name_lower = name.to_string_lossy().to_lowercase();
            if self.skip_filenames.contains(&name_lower) {
                return true;
            }
        }

        // --- Check regex patterns against full path ---
        for re in &self.skip_patterns {
            if re.is_match(&path_str) {
                return true;
            }
        }

        // --- Check glob patterns ---
        // Match against both the full path and the path relative to cwd.
        // This ensures **/target/** works whether paths are absolute or relative.
        if self.skip_globs.is_match(path) {
            return true;
        }
        if let Ok(rel) = path.strip_prefix(std::env::current_dir().unwrap_or_default()) {
            if self.skip_globs.is_match(rel) {
                return true;
            }
        }

        // --- Binary detection ---
        if self.skip_binary && is_binary(path) {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    fn filter_from(cfg: AppConfig) -> Filter {
        Filter::new(&cfg).expect("Filter::new failed")
    }

    fn bare() -> AppConfig {
        AppConfig {
            skip_extensions: vec![],
            skip_patterns: vec![],
            skip_filenames: vec![],
            skip_path_components: vec![],
            skip_globs: vec![],
            skip_binary: false,
            skip_hidden: false,
        }
    }

    // ── Extension filtering ────────────────────────────────────────────────

    #[test]
    fn skips_matching_extension() {
        let cfg = AppConfig {
            skip_extensions: vec!["lock".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("Cargo.lock")));
    }

    #[test]
    fn keeps_non_matching_extension() {
        let cfg = AppConfig {
            skip_extensions: vec!["lock".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(!f.should_skip(Path::new("main.rs")));
    }

    #[test]
    fn extension_check_is_case_insensitive() {
        let cfg = AppConfig {
            skip_extensions: vec!["lock".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("Cargo.LOCK")));
    }

    // ── Filename filtering ─────────────────────────────────────────────────

    #[test]
    fn skips_exact_filename_no_extension() {
        let cfg = AppConfig {
            skip_filenames: vec!["makefile".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("Makefile")));
    }

    #[test]
    fn skips_exact_filename_with_extension() {
        let cfg = AppConfig {
            skip_filenames: vec!["readme".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        // stem "readme" matches
        assert!(f.should_skip(Path::new("README.md")));
    }

    #[test]
    fn filename_check_is_case_insensitive() {
        let cfg = AppConfig {
            skip_filenames: vec!["dockerfile".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("DOCKERFILE")));
    }

    #[test]
    fn keeps_non_matching_filename() {
        let cfg = AppConfig {
            skip_filenames: vec!["makefile".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(!f.should_skip(Path::new("main.rs")));
    }

    // ── Path component filtering ───────────────────────────────────────────

    #[test]
    fn skips_file_inside_blocked_component() {
        let cfg = AppConfig {
            skip_path_components: vec!["node_modules".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("node_modules/lodash/index.js")));
    }

    #[test]
    fn skips_deeply_nested_blocked_component() {
        let cfg = AppConfig {
            skip_path_components: vec![".github".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("project/.github/workflows/ci.yml")));
    }

    #[test]
    fn keeps_file_with_no_blocked_component() {
        let cfg = AppConfig {
            skip_path_components: vec!["node_modules".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(!f.should_skip(Path::new("src/index.js")));
    }

    // ── Hidden file filtering ──────────────────────────────────────────────

    #[test]
    fn skips_hidden_file_when_enabled() {
        let cfg = AppConfig {
            skip_hidden: true,
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new(".env")));
    }

    #[test]
    fn skips_file_inside_hidden_dir() {
        let cfg = AppConfig {
            skip_hidden: true,
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new(".config/something.toml")));
    }

    #[test]
    fn keeps_hidden_file_when_disabled() {
        let cfg = AppConfig {
            skip_hidden: false,
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(!f.should_skip(Path::new(".env")));
    }

    #[test]
    fn dot_single_not_treated_as_hidden() {
        // Path component "." should never be treated as hidden
        let cfg = AppConfig {
            skip_hidden: true,
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(!f.should_skip(Path::new("./src/main.rs")));
    }

    // ── Regex pattern filtering ────────────────────────────────────────────

    #[test]
    fn skips_file_matching_regex_pattern() {
        let cfg = AppConfig {
            skip_patterns: vec![r".*test.*\.rs$".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("src/foo_test.rs")));
    }

    #[test]
    fn regex_pattern_is_case_insensitive() {
        let cfg = AppConfig {
            skip_patterns: vec![r".*test.*\.rs$".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("src/FooTEST.rs")));
    }

    #[test]
    fn keeps_file_not_matching_regex() {
        let cfg = AppConfig {
            skip_patterns: vec![r".*test.*\.rs$".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(!f.should_skip(Path::new("src/main.rs")));
    }

    #[test]
    fn invalid_regex_returns_error() {
        let cfg = AppConfig {
            skip_patterns: vec!["[invalid".into()],
            ..bare()
        };
        assert!(Filter::new(&cfg).is_err());
    }

    // ── Glob pattern filtering ─────────────────────────────────────────────

    #[test]
    fn skips_file_matching_double_star_glob() {
        let cfg = AppConfig {
            skip_globs: vec!["**/target/**".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("my_project/target/debug/dump-dir")));
    }

    #[test]
    fn skips_file_matching_extension_glob() {
        let cfg = AppConfig {
            skip_globs: vec!["**/*.min.js".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("static/app.min.js")));
    }

    #[test]
    fn glob_is_case_insensitive() {
        let cfg = AppConfig {
            skip_globs: vec!["**/TARGET/**".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(f.should_skip(Path::new("project/target/release/bin")));
    }

    #[test]
    fn keeps_file_not_matching_glob() {
        let cfg = AppConfig {
            skip_globs: vec!["**/target/**".into()],
            ..bare()
        };
        let f = filter_from(cfg);
        assert!(!f.should_skip(Path::new("src/main.rs")));
    }

    #[test]
    fn invalid_glob_returns_error() {
        let cfg = AppConfig {
            skip_globs: vec!["[invalid".into()],
            ..bare()
        };
        assert!(Filter::new(&cfg).is_err());
    }

    // ── Default config ─────────────────────────────────────────────────────

    #[test]
    fn default_config_skips_lock_files() {
        let f = filter_from(AppConfig::default());
        assert!(f.should_skip(Path::new("Cargo.lock")));
    }

    #[test]
    fn default_config_skips_snap_files() {
        let f = filter_from(AppConfig::default());
        assert!(f.should_skip(Path::new("tests/snapshots/foo.snap")));
    }

    #[test]
    fn default_config_skips_test_rs_files() {
        let f = filter_from(AppConfig::default());
        assert!(f.should_skip(Path::new("src/foo_test.rs")));
    }

    #[test]
    fn default_config_skips_hidden_files() {
        let f = filter_from(AppConfig::default());
        assert!(f.should_skip(Path::new(".env")));
    }

    #[test]
    fn default_config_keeps_normal_rs_file() {
        let f = filter_from(AppConfig::default());
        // Binary check will pass (no actual file), so should_skip returns false
        // Note: binary check returns false for nonexistent paths
        assert!(!f.should_skip(Path::new("src/main.rs")));
    }
}

/// Sniff the first 8KB of the file to detect binary content.
fn is_binary(path: &Path) -> bool {
    use std::{fs::File, io::Read};

    let Ok(mut f) = File::open(path) else {
        return false; // Can't open → let the printer handle it
    };

    let mut buf = [0u8; 8192];
    let Ok(n) = f.read(&mut buf) else {
        return false;
    };

    // Use the `infer` crate for known binary signatures first
    if let Some(kind) = infer::get(&buf[..n]) {
        let mime = kind.mime_type();
        if !mime.starts_with("text/") {
            return true;
        }
    }

    // Fallback: check for null bytes (classic binary indicator)
    buf[..n].contains(&0u8)
}
