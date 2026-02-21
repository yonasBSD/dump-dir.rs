use std::{fs, path::Path, process::Command};

use anyhow::Result;
use colored::Colorize;

const SEPARATOR: &str = "====================================================";

pub struct Printer {
    file_count: usize,
    line_count: usize,
    skipped_unreadable: usize,
}

impl Printer {
    pub fn new(_summary: bool) -> Self {
        Self {
            file_count: 0,
            line_count: 0,
            skipped_unreadable: 0,
        }
    }

    pub fn print_file(&mut self, path: &Path) -> Result<()> {
        // Check readability before printing the header
        if !is_readable(path) {
            eprintln!(
                "Warning: cannot read '{}' (permission denied)",
                path.display()
            );
            self.skipped_unreadable += 1;
            return Ok(());
        }

        // Print header
        println!("{}", SEPARATOR.bold().blue());
        println!("{}", format!(" FILE: {}", path.display()).bold().blue());
        println!("{}", SEPARATOR.bold().blue());

        // Print content — prefer bat if available
        let lines = if bat_available() {
            print_with_bat(path)
        } else {
            print_with_cat(path)
        };

        println!(); // Blank line between files

        self.file_count += 1;
        if let Some(n) = lines {
            self.line_count += n;
        }

        Ok(())
    }

    pub fn print_summary(&self) {
        println!(
            "{}",
            format!(
                "── Summary: {} file{}, {} line{}{}",
                self.file_count,
                if self.file_count == 1 { "" } else { "s" },
                self.line_count,
                if self.line_count == 1 { "" } else { "s" },
                if self.skipped_unreadable > 0 {
                    format!(", {} unreadable skipped", self.skipped_unreadable)
                } else {
                    String::new()
                }
            )
            .dimmed()
        );
    }
}

/// Returns whether the file can be opened for reading.
fn is_readable(path: &Path) -> bool {
    fs::File::open(path).is_ok()
}

/// Returns true if `bat` is on PATH.
fn bat_available() -> bool {
    which_bat().is_some()
}

fn which_bat() -> Option<String> {
    // Try "bat" then "batcat" (Debian/Ubuntu package name)
    for name in &["bat", "batcat"] {
        if Command::new("which")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(name.to_string());
        }
    }
    None
}

/// Print via bat with line numbers, colors, no pager. Returns line count if knowable.
fn print_with_bat(path: &Path) -> Option<usize> {
    let bat = which_bat()?;
    let status = Command::new(&bat)
        .args(["--style=numbers", "--color=always", "--pager=none"])
        .arg(path)
        .status()
        .ok()?;

    if !status.success() {
        // bat failed (e.g. unsupported file) — fall back to cat
        print_with_cat(path)
    } else {
        // We don't capture bat's output (it streams to stdout), so
        // count lines ourselves for the summary
        count_lines(path)
    }
}

/// Print via plain cat. Returns line count.
fn print_with_cat(path: &Path) -> Option<usize> {
    let content = fs::read_to_string(path).ok()?;
    print!("{content}");
    Some(content.lines().count())
}

fn count_lines(path: &Path) -> Option<usize> {
    let content = fs::read_to_string(path).ok()?;
    Some(content.lines().count())
}
