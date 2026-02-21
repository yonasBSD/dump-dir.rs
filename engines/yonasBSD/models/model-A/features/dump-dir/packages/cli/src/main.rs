/*
 * CLI Entry point.
 *
 * Error handling strategy:
 * - lib functions return Result<_, DumpError> — typed, matchable, ergonomic with `?`
 * - At the boundary here, DumpError is wrapped into LibReport<DumpError> for:
 *     1. miette's fancy terminal rendering (source snippets, help text, codes)
 *     2. Typed introspection via rootcause (branching on specific variants)
 *     3. Structured JSON logging via ReportExt / ApiError
 */

use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use errors_lib::{LibReport, ReportExt, rootcause::Report};
use lib::{DumpError, config, filter, printer, walker};
use miette::Result as MietteResult;

/// Dump directory file contents to terminal, respecting .gitignore
#[derive(Parser, Debug)]
#[command(
    name = "dump-dir",
    version,
    about = "Prints file contents of a directory, git-aware and filter-configurable"
)]
struct Cli {
    /// Paths to dump (files or directories). Defaults to current directory.
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,

    /// Override config: skip extensions (comma-separated, e.g. "snap,lock")
    #[arg(long, value_delimiter = ',', value_name = "EXT")]
    skip_extensions: Option<Vec<String>>,

    /// Override config: skip filename patterns (comma-separated regex)
    #[arg(long, value_delimiter = ',', value_name = "PATTERN")]
    skip_patterns: Option<Vec<String>>,

    /// Include files that would normally be skipped (overrides all filters)
    #[arg(long)]
    no_filter: bool,

    /// Show a summary line count at the end
    #[arg(long)]
    summary: bool,

    /// Path to a local config file (default: ./dump.toml)
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
}

fn run(cli: Cli) -> Result<(), DumpError> {
    // Load layered config: global → local → CLI overrides
    let mut cfg = config::load(cli.config.as_deref())?;

    // Apply CLI overrides on top of config
    if cli.no_filter {
        cfg.skip_extensions.clear();
        cfg.skip_patterns.clear();
        cfg.skip_filenames.clear();
        cfg.skip_path_components.clear();
        cfg.skip_globs.clear();
        cfg.skip_binary = false;
        cfg.skip_hidden = false;
    }
    if let Some(exts) = cli.skip_extensions {
        cfg.skip_extensions = exts;
    }
    if let Some(patterns) = cli.skip_patterns {
        cfg.skip_patterns = patterns;
    }

    // Resolve paths to walk
    let paths: Vec<PathBuf> = if cli.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        cli.paths
    };

    // Validate all paths exist upfront — typed PathNotFound error
    for path in &paths {
        if !path.exists() {
            return Err(DumpError::PathNotFound {
                path: path.display().to_string(),
            });
        }
    }

    let filter = Arc::new(filter::Filter::new(&cfg)?);
    let mut printer = printer::Printer::new(cli.summary);

    for path in &paths {
        let files = walker::collect_files(path, Arc::clone(&filter))?;
        for file in files {
            printer.print_file(&file)?;
        }
    }

    if cli.summary {
        printer.print_summary();
    }

    Ok(())
}

fn main() -> MietteResult<()> {
    // Fancy panic reports for unhandled crashes
    color_eyre::install().expect("Failed to install color-eyre");

    // Respect RUST_LOG for debug tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
        )
        .with_writer(std::io::stderr)
        .compact()
        .init();

    miette::set_panic_hook();

    let cli = Cli::parse();

    // Run the application, wrapping DumpError into LibReport at the boundary.
    match run(cli) {
        Ok(()) => Ok(()),
        Err(err) => {
            // Typed introspection — react to specific variants before rendering
            match &err {
                DumpError::PathNotFound {
                    path,
                } => {
                    eprintln!("Hint: '{}' — did you mean to pass a different path?", path);
                },
                DumpError::ConfigNotFound {
                    path,
                } => {
                    eprintln!("Hint: check --config argument, '{}' not found.", path);
                },
                DumpError::InvalidRegex {
                    pattern, ..
                } => {
                    eprintln!("Hint: invalid regex in config: '{}'", pattern);
                },
                DumpError::InvalidGlob {
                    pattern, ..
                } => {
                    eprintln!("Hint: invalid glob in config: '{}'", pattern);
                },
                _ => {},
            }

            // Wrap into LibReport for miette rendering + structured logging
            let report = LibReport(Report::new(err));
            let api_err = report.to_api_error();
            eprintln!("\n[Diagnostic ID: {}]", api_err.correlation_id);

            // Return as miette::Report for beautiful terminal output
            Err(miette::Report::new(report))
        },
    }
}
