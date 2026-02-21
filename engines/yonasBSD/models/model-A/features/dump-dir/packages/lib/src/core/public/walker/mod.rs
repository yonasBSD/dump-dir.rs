use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use ignore::{DirEntry, WalkBuilder};
use snafu::ResultExt;

use crate::{
    errors::{DumpResult, WalkSnafu},
    filter::Filter,
};

/// Collect all files under `root` that pass the filter, in sorted order.
pub fn collect_files(root: &Path, filter: Arc<Filter>) -> DumpResult<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();

    let filter_dir = Arc::clone(&filter);

    let walker = WalkBuilder::new(root)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .hidden(false)
        .follow_links(false)
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(move |entry: &DirEntry| {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if entry.depth() == 0 {
                    return true;
                }
                !filter_dir.should_skip_dir(entry.path())
            } else {
                true
            }
        })
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    let path = entry.into_path();
                    if !filter.should_skip(&path) {
                        files.push(path);
                    }
                }
            },
            Err(e) => {
                // Log a warning for soft walk errors but don't abort.
                // Only hard errors (e.g. permission denied on root) warrant propagation.
                if e.io_error().map(|io| io.kind()) == Some(std::io::ErrorKind::PermissionDenied) {
                    eprintln!("Warning: {e}");
                } else {
                    return Err(e).context(WalkSnafu);
                }
            },
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::{config::AppConfig, filter::Filter};

    fn bare_filter() -> Arc<Filter> {
        Arc::new(
            Filter::new(&AppConfig {
                skip_extensions: vec![],
                skip_patterns: vec![],
                skip_filenames: vec![],
                skip_path_components: vec![],
                skip_globs: vec![],
                skip_binary: false,
                skip_hidden: false,
            })
            .unwrap(),
        )
    }

    fn arc_filter(cfg: AppConfig) -> Arc<Filter> {
        Arc::new(Filter::new(&cfg).unwrap())
    }

    fn make_files(dir: &TempDir, paths: &[&str]) {
        for p in paths {
            let full = dir.path().join(p);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(&full, format!("content of {p}")).unwrap();
        }
    }

    fn filenames(files: &[PathBuf]) -> Vec<String> {
        let mut names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn collects_all_files_in_flat_dir() {
        let dir = TempDir::new().unwrap();
        make_files(&dir, &["a.rs", "b.rs", "c.toml"]);
        let files = collect_files(dir.path(), bare_filter()).unwrap();
        assert_eq!(filenames(&files), vec!["a.rs", "b.rs", "c.toml"]);
    }

    #[test]
    fn collects_files_recursively() {
        let dir = TempDir::new().unwrap();
        make_files(&dir, &["src/main.rs", "src/lib.rs", "README.md"]);
        let files = collect_files(dir.path(), bare_filter()).unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn output_is_sorted() {
        let dir = TempDir::new().unwrap();
        make_files(&dir, &["z.rs", "a.rs", "m.rs"]);
        let files = collect_files(dir.path(), bare_filter()).unwrap();
        let names = filenames(&files);
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn returns_empty_for_empty_dir() {
        let dir = TempDir::new().unwrap();
        let files = collect_files(dir.path(), bare_filter()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn single_file_path_works() {
        let dir = TempDir::new().unwrap();
        make_files(&dir, &["only.rs"]);
        let path = dir.path().join("only.rs");
        let files = collect_files(&path, bare_filter()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "only.rs");
    }

    #[test]
    fn filtered_extensions_are_excluded() {
        let dir = TempDir::new().unwrap();
        make_files(&dir, &["Cargo.lock", "src/main.rs"]);
        let filter = arc_filter(AppConfig {
            skip_extensions: vec!["lock".into()],
            skip_binary: false,
            skip_hidden: false,
            skip_patterns: vec![],
            skip_filenames: vec![],
            skip_path_components: vec![],
            skip_globs: vec![],
        });
        let files = collect_files(dir.path(), filter).unwrap();
        assert_eq!(filenames(&files), vec!["main.rs"]);
    }

    #[test]
    fn glob_filter_excludes_target_dir() {
        let dir = TempDir::new().unwrap();
        make_files(&dir, &["src/main.rs", "target/debug/dump-dir"]);
        let filter = arc_filter(AppConfig {
            skip_globs: vec!["**/target/**".into()],
            skip_binary: false,
            skip_hidden: false,
            skip_extensions: vec![],
            skip_patterns: vec![],
            skip_filenames: vec![],
            skip_path_components: vec![],
        });
        let files = collect_files(dir.path(), filter).unwrap();
        assert_eq!(filenames(&files), vec!["main.rs"]);
    }

    #[test]
    fn hidden_files_excluded_when_skip_hidden_true() {
        let dir = TempDir::new().unwrap();
        make_files(&dir, &[".env", "main.rs"]);
        let filter = arc_filter(AppConfig {
            skip_hidden: true,
            skip_binary: false,
            skip_extensions: vec![],
            skip_patterns: vec![],
            skip_filenames: vec![],
            skip_path_components: vec![],
            skip_globs: vec![],
        });
        let files = collect_files(dir.path(), filter).unwrap();
        assert_eq!(filenames(&files), vec!["main.rs"]);
    }

    #[test]
    fn respects_gitignore() {
        let dir = TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .ok();
        make_files(&dir, &["src/main.rs", "ignored.log"]);
        fs::write(dir.path().join(".gitignore"), "*.log\n").unwrap();
        let files = collect_files(dir.path(), bare_filter()).unwrap();
        let names = filenames(&files);
        assert!(!names.contains(&"ignored.log".to_string()));
        assert!(names.contains(&"main.rs".to_string()));
    }
}
