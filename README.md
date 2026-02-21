# dump-dir

Dumps the contents of a directory to your terminal — git-aware, filter-configurable, and `bat`-powered when available.

## Install

```sh
cargo install --path .
```

## Usage

```sh
# Dump the current directory (respects .gitignore)
dump-dir

# Dump a specific path
dump-dir src/

# Dump multiple paths
dump-dir src/ tests/

# Override skip rules inline
dump-dir --skip-extensions snap,lock,new
dump-dir --skip-patterns '.*test.*\.rs$'

# Disable all filtering
dump-dir --no-filter

# Show a summary at the end
dump-dir --summary

# Use a custom config file
dump-dir --config /path/to/myconfig.toml
```

## Configuration

`dump-dir` uses **layered configuration** — later layers override earlier ones:

| Layer | Path | Notes |
|-------|------|-------|
| 1. Defaults | (built-in) | Always applied as the base |
| 2. Global | `~/.config/dump-dir/config.toml` | User-wide settings |
| 3. Local | `./dump.toml` | Per-project settings |
| 4. CLI flags | `--skip-extensions`, etc. | One-off overrides |

### Config file format

```toml
# ~/.config/dump-dir/config.toml  OR  ./dump.toml

# Extensions to skip (no leading dot)
skip_extensions = ["snap", "lock", "new", "gitignore", "orig", "bak", "swp"]

# Regex patterns matched against full file path (case-insensitive)
skip_patterns = [".*test.*\\.rs$"]

# Exact filenames to skip (case-insensitive)
skip_filenames = ["license", "readme", "changelog", "makefile", "dockerfile"]

# Any path component matching these causes the file to be skipped
skip_path_components = [".github", ".git", "node_modules", ".direnv"]

# Skip binary files (detected via MIME sniffing + null byte check)
skip_binary = true

# Skip hidden files/dirs (any path component starting with '.')
skip_hidden = true
```

> **Note**: Arrays replace rather than merge across layers. If you define
> `skip_extensions` in your local `dump.toml`, it fully replaces the global
> list — so include everything you want.

## Output

When `bat` (or `batcat`) is on your PATH, files are printed with syntax
highlighting and line numbers. Otherwise, plain `cat` output is used.

Headers are printed in bold blue between each file.

## How it works

- Inside a git repo: uses the [`ignore`](https://docs.rs/ignore) crate, which
  natively reads `.gitignore`, `.ignore`, and global git excludes.
- Outside a git repo: standard recursive directory walk.
- Binary detection: sniffs the first 8KB of each file using
  [`infer`](https://docs.rs/infer) + null byte scanning.
