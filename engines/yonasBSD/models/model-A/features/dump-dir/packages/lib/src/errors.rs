/*
 * Structured error definitions for the dump-dir library.
 *
 * Each variant maps to a specific failure mode in the application.
 * snafu generates the context structs and From impls.
 * miette provides the diagnostic metadata: codes, help text, source spans.
 */

use miette::Diagnostic;
use snafu::prelude::*;

#[derive(Debug, Snafu, Diagnostic)]
#[snafu(visibility(pub))]
pub enum DumpError {
    // ── Config ────────────────────────────────────────────────────────────
    /// Failed to build the layered config (file parse error, bad TOML, etc.)
    #[snafu(display("Failed to load configuration: {source}"))]
    #[diagnostic(
        code(dump_dir::config::load_failed),
        help("Check that your config file is valid TOML and all fields have correct types.")
    )]
    ConfigLoad { source: config::ConfigError },

    /// User passed --config but the file doesn't exist.
    #[snafu(display("Config file not found: {path}"))]
    #[diagnostic(
        code(dump_dir::config::not_found),
        help("Pass a path to an existing .toml file, or omit --config to use defaults.")
    )]
    ConfigNotFound { path: String },

    // ── Filter construction ───────────────────────────────────────────────
    /// A regex pattern in skip_patterns failed to compile.
    #[snafu(display("Invalid regex pattern '{pattern}': {source}"))]
    #[diagnostic(
        code(dump_dir::filter::invalid_regex),
        help("Check your skip_patterns config. Patterns must be valid Rust regex syntax.")
    )]
    InvalidRegex {
        pattern: String,
        source: regex::Error,
    },

    /// A glob pattern in skip_globs failed to compile.
    #[snafu(display("Invalid glob pattern '{pattern}': {source}"))]
    #[diagnostic(
        code(dump_dir::filter::invalid_glob),
        help("Check your skip_globs config. Use ** for directory wildcards, * for filenames.")
    )]
    InvalidGlob {
        pattern: String,
        source: globset::Error,
    },

    /// The glob set itself failed to build (very rare — usually a memory issue).
    #[snafu(display("Failed to build glob set: {source}"))]
    #[diagnostic(code(dump_dir::filter::glob_set_build_failed))]
    GlobSetBuild { source: globset::Error },

    // ── Path / IO ─────────────────────────────────────────────────────────
    /// A path provided by the user does not exist on disk.
    #[snafu(display("Path does not exist: {path}"))]
    #[diagnostic(
        code(dump_dir::path::not_found),
        help("Check that the path is correct and the file or directory exists.")
    )]
    PathNotFound { path: String },

    /// An IO error occurred reading a file or directory.
    #[snafu(display("IO error reading '{path}': {source}"))]
    #[diagnostic(
        code(dump_dir::io::read_failed),
        help("Check file permissions and that the path is accessible.")
    )]
    Io {
        path: String,
        source: std::io::Error,
    },

    // ── Walker ────────────────────────────────────────────────────────────
    /// The ignore crate emitted a walk error for an entry.
    #[snafu(display("Walk error: {source}"))]
    #[diagnostic(
        code(dump_dir::walker::walk_error),
        help("A filesystem entry could not be accessed during directory traversal.")
    )]
    Walk { source: ignore::Error },
}

/// Convenience Result alias for the dump-dir library.
/// Internal functions return this directly; the CLI wraps it into LibReport at the boundary.
pub type DumpResult<T> = std::result::Result<T, DumpError>;
