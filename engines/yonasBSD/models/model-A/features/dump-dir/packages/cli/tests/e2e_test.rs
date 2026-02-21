/// End-to-end tests: spawn the real `dump-dir` binary and assert on stdout/stderr.
/// These test the full user-facing behaviour.
use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// ── helpers ────────────────────────────────────────────────────────────────

#[allow(deprecated)]
fn cmd() -> Command {
    Command::cargo_bin("dump-dir").unwrap()
}

fn make(dir: &TempDir, paths: &[(&str, &str)]) {
    for (path, content) in paths {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }
}

fn no_filter_toml() -> &'static str {
    r#"
skip_extensions = []
skip_patterns = []
skip_filenames = []
skip_path_components = []
skip_globs = []
skip_binary = false
skip_hidden = false
"#
}

// ── Basic output ───────────────────────────────────────────────────────────

#[test]
fn prints_file_header_and_content() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[("hello.txt", "hello world")]);
    fs::write(dir.path().join("dump.toml"), no_filter_toml()).unwrap();

    cmd()
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("dump.toml"))
        .assert()
        .success()
        .stdout(predicate::str::contains("FILE: "))
        .stdout(predicate::str::contains("hello world"))
        .stdout(predicate::str::contains("===="));
}

#[test]
fn exits_zero_on_empty_directory() {
    let dir = TempDir::new().unwrap();
    cmd().arg(dir.path()).assert().success();
}

#[test]
fn exits_nonzero_for_nonexistent_path() {
    cmd()
        .arg("/absolutely/does/not/exist/ever")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

// ── --no-filter ────────────────────────────────────────────────────────────

#[test]
fn no_filter_flag_shows_lock_files() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[("Cargo.lock", "[package]")]);

    cmd()
        .arg(dir.path())
        .arg("--no-filter")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cargo.lock"));
}

#[test]
fn no_filter_flag_shows_hidden_files() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[(".env", "SECRET=hunter2")]);

    cmd()
        .arg(dir.path())
        .arg("--no-filter")
        .assert()
        .success()
        .stdout(predicate::str::contains(".env"));
}

// ── --skip-extensions ─────────────────────────────────────────────────────

#[test]
fn skip_extensions_flag_excludes_files() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[("main.rs", "fn main() {}"), ("notes.txt", "hello")]);
    fs::write(dir.path().join("dump.toml"), no_filter_toml()).unwrap();

    cmd()
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("dump.toml"))
        .arg("--skip-extensions")
        .arg("txt")
        .assert()
        .success()
        .stdout(predicate::str::contains("main.rs"))
        .stdout(predicate::str::contains("notes.txt").not());
}

// ── --summary ─────────────────────────────────────────────────────────────

#[test]
fn summary_flag_prints_summary_line() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[("a.txt", "line1\nline2"), ("b.txt", "line3")]);
    fs::write(dir.path().join("dump.toml"), no_filter_toml()).unwrap();

    cmd()
        .arg(dir.path())
        .arg("--config")
        .arg(dir.path().join("dump.toml"))
        .arg("--summary")
        .assert()
        .success()
        .stdout(predicate::str::contains("Summary:"))
        .stdout(predicate::str::contains("file"));
}

// ── --config ───────────────────────────────────────────────────────────────

#[test]
fn explicit_config_path_is_used() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[
        ("Cargo.lock", "[lock]"),
        ("main.rs", "fn main() {}"),
    ]);

    // Config that skips .rs files but keeps lock files
    let config_content = r#"
skip_extensions = ["rs"]
skip_patterns = []
skip_filenames = []
skip_path_components = []
skip_globs = []
skip_binary = false
skip_hidden = false
"#;
    let config_path = dir.path().join("custom.toml");
    fs::write(&config_path, config_content).unwrap();

    cmd()
        .arg(dir.path())
        .arg("--config")
        .arg(&config_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Cargo.lock"))
        .stdout(predicate::str::contains("main.rs").not());
}

#[test]
fn nonexistent_explicit_config_exits_with_error() {
    let dir = TempDir::new().unwrap();
    cmd()
        .arg(dir.path())
        .arg("--config")
        .arg("/no/such/config.toml")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Config file not found"));
}

// ── Multiple paths ─────────────────────────────────────────────────────────

#[test]
fn multiple_path_args_all_printed() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[("alpha/a.txt", "aaa"), ("beta/b.txt", "bbb")]);
    fs::write(dir.path().join("dump.toml"), no_filter_toml()).unwrap();

    cmd()
        .arg(dir.path().join("alpha"))
        .arg(dir.path().join("beta"))
        .arg("--config")
        .arg(dir.path().join("dump.toml"))
        .assert()
        .success()
        .stdout(predicate::str::contains("aaa"))
        .stdout(predicate::str::contains("bbb"));
}

// ── Single file path arg ───────────────────────────────────────────────────

#[test]
fn single_file_arg_prints_that_file() {
    let dir = TempDir::new().unwrap();
    make(&dir, &[("only.txt", "just this")]);
    fs::write(dir.path().join("dump.toml"), no_filter_toml()).unwrap();

    cmd()
        .arg(dir.path().join("only.txt"))
        .arg("--config")
        .arg(dir.path().join("dump.toml"))
        .assert()
        .success()
        .stdout(predicate::str::contains("just this"));
}

// ── Glob via config file ───────────────────────────────────────────────────

#[test]
fn glob_in_config_file_excludes_target() {
    let dir = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap(); // separate dir so config isn't walked
    make(&dir, &[
        ("src/main.rs", "fn main() {}"),
        ("target/debug/binary", "ELF"),
    ]);
    let config_content = r#"
skip_extensions = []
skip_patterns = []
skip_filenames = []
skip_path_components = []
skip_globs = ["**/target/**"]
skip_binary = false
skip_hidden = false
"#;
    let config_path = config_dir.path().join("dump.toml");
    fs::write(&config_path, config_content).unwrap();

    cmd()
        .arg(dir.path())
        .arg("--config")
        .arg(&config_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("main.rs"))
        .stdout(predicate::str::contains("binary").not());
}
