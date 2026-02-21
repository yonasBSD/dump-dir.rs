use std::path::Path;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::Regex;
use snafu::ResultExt;

use crate::{
    config::AppConfig,
    errors::{DumpResult, GlobSetBuildSnafu, InvalidGlobSnafu, InvalidRegexSnafu},
};

#[derive(Debug)]
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
    pub fn new(cfg: &AppConfig) -> DumpResult<Self> {
        let skip_patterns = cfg
            .skip_patterns
            .iter()
            .map(|p| {
                Regex::new(&format!("(?i){p}")).context(InvalidRegexSnafu {
                    pattern: p.clone(),
                })
            })
            .collect::<DumpResult<Vec<_>>>()?;

        let mut glob_builder = GlobSetBuilder::new();
        for pattern in &cfg.skip_globs {
            let glob = GlobBuilder::new(pattern)
                .case_insensitive(true)
                .literal_separator(true)
                .build()
                .context(InvalidGlobSnafu {
                    pattern: pattern.clone(),
                })?;
            glob_builder.add(glob);
        }
        let skip_globs = glob_builder.build().context(GlobSetBuildSnafu)?;

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
    pub fn should_skip_dir(&self, path: &Path) -> bool {
        if let Some(name) = path.file_name() {
            let name_lower = name.to_string_lossy().to_lowercase();

            if self.skip_hidden && name_lower.starts_with('.') {
                return true;
            }
            if self.skip_path_components.contains(&name_lower) {
                return true;
            }
        }

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

        for component in path.components() {
            let c = component.as_os_str().to_string_lossy().to_lowercase();
            if self.skip_path_components.contains(&c) {
                return true;
            }
        }

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

        if let Some(ext) = path.extension() {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            if self.skip_extensions.contains(&ext_lower) {
                return true;
            }
        }

        if let Some(name) = path.file_stem() {
            let name_lower = name.to_string_lossy().to_lowercase();
            if self.skip_filenames.contains(&name_lower) {
                return true;
            }
        }
        if let Some(name) = path.file_name() {
            let name_lower = name.to_string_lossy().to_lowercase();
            if self.skip_filenames.contains(&name_lower) {
                return true;
            }
        }

        for re in &self.skip_patterns {
            if re.is_match(&path_str) {
                return true;
            }
        }

        if self.skip_globs.is_match(path) {
            return true;
        }
        if let Ok(rel) = path.strip_prefix(std::env::current_dir().unwrap_or_default()) {
            if self.skip_globs.is_match(rel) {
                return true;
            }
        }

        if self.skip_binary && is_binary(path) {
            return true;
        }

        false
    }
}

/// Sniff the first 8KB of the file to detect binary content.
fn is_binary(path: &Path) -> bool {
    use std::{fs::File, io::Read};

    let Ok(mut f) = File::open(path) else {
        return false;
    };

    let mut buf = [0u8; 8192];
    let Ok(n) = f.read(&mut buf) else {
        return false;
    };

    if let Some(kind) = infer::get(&buf[..n]) {
        let mime = kind.mime_type();
        if !mime.starts_with("text/") {
            return true;
        }
    }

    buf[..n].contains(&0u8)
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

    #[test]
    fn skips_matching_extension() {
        let f = filter_from(AppConfig {
            skip_extensions: vec!["lock".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("Cargo.lock")));
    }

    #[test]
    fn keeps_non_matching_extension() {
        let f = filter_from(AppConfig {
            skip_extensions: vec!["lock".into()],
            ..bare()
        });
        assert!(!f.should_skip(Path::new("main.rs")));
    }

    #[test]
    fn extension_check_is_case_insensitive() {
        let f = filter_from(AppConfig {
            skip_extensions: vec!["lock".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("Cargo.LOCK")));
    }

    #[test]
    fn skips_exact_filename_no_extension() {
        let f = filter_from(AppConfig {
            skip_filenames: vec!["makefile".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("Makefile")));
    }

    #[test]
    fn skips_exact_filename_with_extension() {
        let f = filter_from(AppConfig {
            skip_filenames: vec!["readme".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("README.md")));
    }

    #[test]
    fn filename_check_is_case_insensitive() {
        let f = filter_from(AppConfig {
            skip_filenames: vec!["dockerfile".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("DOCKERFILE")));
    }

    #[test]
    fn keeps_non_matching_filename() {
        let f = filter_from(AppConfig {
            skip_filenames: vec!["makefile".into()],
            ..bare()
        });
        assert!(!f.should_skip(Path::new("main.rs")));
    }

    #[test]
    fn skips_file_inside_blocked_component() {
        let f = filter_from(AppConfig {
            skip_path_components: vec!["node_modules".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("node_modules/lodash/index.js")));
    }

    #[test]
    fn skips_deeply_nested_blocked_component() {
        let f = filter_from(AppConfig {
            skip_path_components: vec![".github".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("project/.github/workflows/ci.yml")));
    }

    #[test]
    fn keeps_file_with_no_blocked_component() {
        let f = filter_from(AppConfig {
            skip_path_components: vec!["node_modules".into()],
            ..bare()
        });
        assert!(!f.should_skip(Path::new("src/index.js")));
    }

    #[test]
    fn skips_hidden_file_when_enabled() {
        let f = filter_from(AppConfig {
            skip_hidden: true,
            ..bare()
        });
        assert!(f.should_skip(Path::new(".env")));
    }

    #[test]
    fn skips_file_inside_hidden_dir() {
        let f = filter_from(AppConfig {
            skip_hidden: true,
            ..bare()
        });
        assert!(f.should_skip(Path::new(".config/something.toml")));
    }

    #[test]
    fn keeps_hidden_file_when_disabled() {
        let f = filter_from(AppConfig {
            skip_hidden: false,
            ..bare()
        });
        assert!(!f.should_skip(Path::new(".env")));
    }

    #[test]
    fn dot_single_not_treated_as_hidden() {
        let f = filter_from(AppConfig {
            skip_hidden: true,
            ..bare()
        });
        assert!(!f.should_skip(Path::new("./src/main.rs")));
    }

    #[test]
    fn skips_file_matching_regex_pattern() {
        let f = filter_from(AppConfig {
            skip_patterns: vec![r".*test.*\.rs$".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("src/foo_test.rs")));
    }

    #[test]
    fn regex_pattern_is_case_insensitive() {
        let f = filter_from(AppConfig {
            skip_patterns: vec![r".*test.*\.rs$".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("src/FooTEST.rs")));
    }

    #[test]
    fn keeps_file_not_matching_regex() {
        let f = filter_from(AppConfig {
            skip_patterns: vec![r".*test.*\.rs$".into()],
            ..bare()
        });
        assert!(!f.should_skip(Path::new("src/main.rs")));
    }

    #[test]
    fn invalid_regex_returns_typed_error() {
        let result = Filter::new(&AppConfig {
            skip_patterns: vec!["[invalid".into()],
            ..bare()
        });
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::errors::DumpError::InvalidRegex { .. }
        ));
    }

    #[test]
    fn skips_file_matching_double_star_glob() {
        let f = filter_from(AppConfig {
            skip_globs: vec!["**/target/**".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("my_project/target/debug/dump-dir")));
    }

    #[test]
    fn skips_file_matching_extension_glob() {
        let f = filter_from(AppConfig {
            skip_globs: vec!["**/*.min.js".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("static/app.min.js")));
    }

    #[test]
    fn glob_is_case_insensitive() {
        let f = filter_from(AppConfig {
            skip_globs: vec!["**/TARGET/**".into()],
            ..bare()
        });
        assert!(f.should_skip(Path::new("project/target/release/bin")));
    }

    #[test]
    fn keeps_file_not_matching_glob() {
        let f = filter_from(AppConfig {
            skip_globs: vec!["**/target/**".into()],
            ..bare()
        });
        assert!(!f.should_skip(Path::new("src/main.rs")));
    }

    #[test]
    fn invalid_glob_returns_typed_error() {
        let result = Filter::new(&AppConfig {
            skip_globs: vec!["[invalid".into()],
            ..bare()
        });
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::errors::DumpError::InvalidGlob { .. }
        ));
    }

    #[test]
    fn default_config_skips_lock_files() {
        assert!(filter_from(AppConfig::default()).should_skip(Path::new("Cargo.lock")));
    }

    #[test]
    fn default_config_skips_snap_files() {
        assert!(
            filter_from(AppConfig::default()).should_skip(Path::new("tests/snapshots/foo.snap"))
        );
    }

    #[test]
    fn default_config_skips_test_rs_files() {
        assert!(filter_from(AppConfig::default()).should_skip(Path::new("src/foo_test.rs")));
    }

    #[test]
    fn default_config_skips_hidden_files() {
        assert!(filter_from(AppConfig::default()).should_skip(Path::new(".env")));
    }

    #[test]
    fn default_config_keeps_normal_rs_file() {
        assert!(!filter_from(AppConfig::default()).should_skip(Path::new("src/main.rs")));
    }
}
