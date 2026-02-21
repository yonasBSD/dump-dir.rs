/// Integration tests: exercise Filter + walker::collect_files together
/// without spawning a subprocess. These test the full internal pipeline
/// as it would run in production.
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

fn collected_names(dir: &TempDir, cfg: AppConfig) -> Vec<String> {
    let filter = std::sync::Arc::new(Filter::new(&cfg).unwrap());
    let mut files = collect_files(dir.path(), filter).unwrap();
    files.sort();
    files
        .iter()
        .map(|p| {
            p.strip_prefix(dir.path())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/") // normalise Windows paths in tests
        })
        .collect()
}

fn no_filter_cfg() -> AppConfig {
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

// ── Extension + filename combinations ─────────────────────────────────────

#[test]
fn extension_and_filename_filters_combine() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("src/main.rs", "fn main() {}"),
        ("Cargo.lock", "[deps]"),
        ("README.md", "# hi"),
        ("Makefile", "build:"),
    ]);
    let cfg = AppConfig {
        skip_extensions: vec!["lock".into(), "md".into()],
        skip_filenames: vec!["makefile".into()],
        ..no_filter_cfg()
    };
    let names = collected_names(&dir, cfg);
    assert_eq!(names, vec!["src/main.rs"]);
}

// ── Glob + path component filters ─────────────────────────────────────────

#[test]
fn glob_and_component_filters_combine() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("src/main.rs", "fn main() {}"),
        ("target/debug/binary", "\x00binary"),
        ("node_modules/lib/index.js", "module.exports = {}"),
        ("dist/app.min.js", "!function(){}()"),
    ]);
    let cfg = AppConfig {
        skip_globs: vec!["**/target/**".into(), "**/*.min.js".into()],
        skip_path_components: vec!["node_modules".into()],
        ..no_filter_cfg()
    };
    let names = collected_names(&dir, cfg);
    assert_eq!(names, vec!["src/main.rs"]);
}

// ── Hidden file handling ───────────────────────────────────────────────────

#[test]
fn hidden_dir_entirely_excluded() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("src/main.rs", "fn main() {}"),
        (".github/workflows/ci.yml", "on: push"),
        (".env", "SECRET=hunter2"),
    ]);
    let cfg = AppConfig {
        skip_hidden: true,
        ..no_filter_cfg()
    };
    let names = collected_names(&dir, cfg);
    assert_eq!(names, vec!["src/main.rs"]);
}

#[test]
fn hidden_files_included_when_skip_hidden_false() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("src/main.rs", "fn main() {}"),
        (".env", "SECRET=x"),
    ]);
    let cfg = AppConfig {
        skip_hidden: false,
        ..no_filter_cfg()
    };
    let names = collected_names(&dir, cfg);
    assert!(names.contains(&".env".to_string()));
}

// ── Regex pattern filtering ────────────────────────────────────────────────

#[test]
fn regex_skips_test_files_keeps_normal() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("src/main.rs", "fn main() {}"),
        ("src/main_test.rs", "#[test]"),
        ("src/foo.rs", "pub fn foo() {}"),
    ]);
    let cfg = AppConfig {
        skip_patterns: vec![r".*test.*\.rs$".into()],
        ..no_filter_cfg()
    };
    let names = collected_names(&dir, cfg);
    assert!(!names.iter().any(|n| n.contains("test")));
    assert!(names.contains(&"src/foo.rs".to_string()));
    assert!(names.contains(&"src/main.rs".to_string()));
}

// ── Default config smoke test ──────────────────────────────────────────────

#[test]
fn default_config_filters_expected_files() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("src/main.rs", "fn main() {}"),
        ("Cargo.lock", "[lock]"),
        ("src/foo_test.rs", "#[test] fn it_works() {}"),
        (".env", "SECRET=x"),
        ("README.md", "# readme"),
    ]);
    let names = collected_names(&dir, AppConfig::default());
    // Only src/main.rs should survive the default filters
    // (.env hidden, Cargo.lock extension, foo_test.rs pattern, README.md filename)
    assert_eq!(names, vec!["src/main.rs"]);
}

// ── Empty directory ────────────────────────────────────────────────────────

#[test]
fn empty_directory_returns_empty_vec() {
    let dir = TempDir::new().unwrap();
    let names = collected_names(&dir, no_filter_cfg());
    assert!(names.is_empty());
}

// ── No-filter: all files visible ──────────────────────────────────────────

#[test]
fn no_filter_returns_everything() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("a.lock", ""),
        (".hidden", ""),
        ("src/test_foo.rs", ""),
        ("Makefile", ""),
    ]);
    let names = collected_names(&dir, no_filter_cfg());
    assert_eq!(names.len(), 4);
}
