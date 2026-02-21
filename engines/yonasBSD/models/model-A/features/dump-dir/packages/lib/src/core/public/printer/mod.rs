use std::{fs, path::Path, process::Command};

use colored::Colorize;
use snafu::ResultExt;

use crate::errors::{DumpResult, IoSnafu};

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

    pub fn print_file(&mut self, path: &Path) -> DumpResult<()> {
        if !is_readable(path) {
            eprintln!(
                "Warning: cannot read '{}' (permission denied)",
                path.display()
            );
            self.skipped_unreadable += 1;
            return Ok(());
        }

        println!("{}", SEPARATOR.bold().blue());
        println!("{}", format!(" FILE: {}", path.display()).bold().blue());
        println!("{}", SEPARATOR.bold().blue());

        let lines = if bat_available() {
            print_with_bat(path)
        } else {
            print_with_cat(path).context(IoSnafu {
                path: path.display().to_string(),
            })?
        };

        println!();

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

fn is_readable(path: &Path) -> bool {
    fs::File::open(path).is_ok()
}

fn bat_available() -> bool {
    which_bat().is_some()
}

fn which_bat() -> Option<String> {
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

fn print_with_bat(path: &Path) -> Option<usize> {
    let bat = which_bat()?;
    let status = Command::new(&bat)
        .args(["--style=numbers", "--color=always", "--pager=none"])
        .arg(path)
        .status()
        .ok()?;

    if !status.success() {
        print_with_cat(path).ok()?
    } else {
        count_lines(path)
    }
}

fn print_with_cat(path: &Path) -> std::io::Result<Option<usize>> {
    let content = fs::read_to_string(path)?;
    print!("{content}");
    Ok(Some(content.lines().count()))
}

fn count_lines(path: &Path) -> Option<usize> {
    let content = fs::read_to_string(path).ok()?;
    Some(content.lines().count())
}
