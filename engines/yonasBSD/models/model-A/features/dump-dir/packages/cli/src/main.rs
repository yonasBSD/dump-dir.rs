use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use lib::{config, filter, printer, walker};

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

fn main() -> Result<()> {
    let cli = Cli::parse();

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

    // Validate all paths exist upfront
    for path in &paths {
        if !path.exists() {
            anyhow::bail!("Path does not exist: {}", path.display());
        }
    }

    let filter = std::sync::Arc::new(filter::Filter::new(&cfg)?);
    let mut printer = printer::Printer::new(cli.summary);

    for path in &paths {
        let files = walker::collect_files(path, std::sync::Arc::clone(&filter))?;
        for file in files {
            printer.print_file(&file)?;
        }
    }

    if cli.summary {
        printer.print_summary();
    }

    Ok(())
}
