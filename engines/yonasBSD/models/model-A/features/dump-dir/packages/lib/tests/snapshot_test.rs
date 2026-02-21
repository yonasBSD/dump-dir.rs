/// Snapshot tests using `insta`.
///
/// On first run (or after `cargo insta review`), snapshots are written to
/// tests/snapshots/. Commit those files — they are the source of truth.
///
/// To update snapshots after intentional output changes:
///   cargo insta review
/// or:
///   INSTA_UPDATE=always cargo test
use std::fs;

use lib::{config::AppConfig, filter::Filter, walker::collect_files};
use tempfile::TempDir;

// ── helpers ────────────────────────────────────────────────────────────────

fn make(dir: &TempDir, paths: &[(&str, &str)]) {
    for (path, content) in paths {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }
}

/// Collect files and return their relative paths as a sorted Vec<String>,
/// normalised so snapshots are stable across platforms and temp dir locations.
fn relative_paths(dir: &TempDir, cfg: AppConfig) -> Vec<String> {
    let filter = std::sync::Arc::new(Filter::new(&cfg).unwrap());
    let mut files = collect_files(dir.path(), filter).unwrap();
    files.sort();
    files
        .iter()
        .map(|p| {
            p.strip_prefix(dir.path())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

fn no_filter() -> AppConfig {
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

// ── File list snapshots ────────────────────────────────────────────────────

/// Snapshot of exactly which files survive the default config.
#[test]
fn snap_default_config_file_list() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("src/main.rs", "fn main() {}"),
        ("src/lib.rs", "pub fn lib() {}"),
        ("src/main_test.rs", "#[test] fn it() {}"),
        ("Cargo.lock", "[lock]"),
        ("Cargo.toml", "[package]"),
        (".env", "SECRET=x"),
        ("README.md", "# readme"),
        ("Makefile", "build:"),
        (".github/workflows/ci.yml", "on: push"),
    ]);
    let files = relative_paths(&dir, AppConfig::default());
    insta::assert_yaml_snapshot!(files);
}

/// Snapshot of files collected with no filters at all.
#[test]
fn snap_no_filter_file_list() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("src/main.rs", "fn main() {}"),
        ("Cargo.lock", "[lock]"),
        (".env", "SECRET=x"),
        ("README.md", "# readme"),
    ]);
    let files = relative_paths(&dir, no_filter());
    insta::assert_yaml_snapshot!(files);
}

/// Snapshot of glob exclusion results.
#[test]
fn snap_glob_exclusion() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("src/main.rs", "fn main() {}"),
        ("target/debug/dump-dir", "binary"),
        ("target/release/dump-dir", "binary"),
        ("dist/app.min.js", "!fn(){}()"),
        ("static/app.js", "console.log('hi')"),
    ]);
    let cfg = AppConfig {
        skip_globs: vec!["**/target/**".into(), "**/*.min.js".into()],
        ..no_filter()
    };
    let files = relative_paths(&dir, cfg);
    insta::assert_yaml_snapshot!(files);
}

// ── AppConfig snapshot ─────────────────────────────────────────────────────

/// Snapshot of the full default AppConfig — catches any accidental changes
/// to defaults (new fields, changed values, etc.).
#[test]
fn snap_default_app_config() {
    let cfg = AppConfig::default();
    insta::assert_toml_snapshot!(cfg);
}

// ── Filter decision snapshots ──────────────────────────────────────────────

/// Snapshot the skip/keep decision for a fixed set of paths under default config.
/// This makes regressions in filter logic immediately visible.
#[test]
fn snap_filter_decisions_default_config() {
    let filter = Filter::new(&AppConfig::default()).unwrap();

    let paths = vec![
        "src/main.rs",
        "src/lib.rs",
        "src/main_test.rs",
        "Cargo.lock",
        "Cargo.toml",
        ".env",
        "README.md",
        "Makefile",
        ".github/workflows/ci.yml",
        "node_modules/lodash/index.js",
        "target/debug/dump-dir",
        "dist/app.min.js",
    ];

    let decisions: Vec<(&str, bool)> = paths
        .iter()
        .map(|p| (*p, filter.should_skip(std::path::Path::new(p))))
        .collect();

    insta::assert_yaml_snapshot!(decisions);
}
