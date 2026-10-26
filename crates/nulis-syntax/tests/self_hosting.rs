//! Nulis opens its own repository well: every text file gets a language,
//! except files that really are plain text, and the files of its own
//! technology stack get the right one. (Plan 4, the owner's goal.)

use nulis_syntax::LanguageRegistry;
use std::path::{Path, PathBuf};

/// Directories that hold build output or downloaded packages, not sources.
const SKIPPED_DIRECTORIES: &[&str] = &["target", "node_modules", ".git", "dist", "gen"];

fn files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("read directory") {
            let entry = entry.expect("entry");
            let path = entry.path();
            let kind = entry.file_type().expect("type");
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind.is_dir() {
                if !SKIPPED_DIRECTORIES.contains(&name.as_str()) {
                    pending.push(path);
                }
            } else if kind.is_file() && name != ".DS_Store" {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Plain Text is right for these.
fn plain_by_nature(relative: &str) -> bool {
    let name = relative.rsplit('/').next().unwrap_or(relative);
    name.contains("LICENSE")
        // A bare version number, such as `24`, for Node version managers.
        || name == ".nvmrc"
        || Path::new(name).extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("txt") || extension.eq_ignore_ascii_case("log")
        })
        || relative.starts_with("fuzz/corpus/")
        || relative.starts_with("fuzz/artifacts/")
}

#[test]
fn the_repository_opens_with_the_right_languages() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry = LanguageRegistry::with_builtins();
    let mut plain = Vec::new();
    let mut checked = 0;
    for path in files(&root) {
        let bytes = std::fs::read(&path).expect("read");
        if bytes.iter().take(8 * 1024).any(|byte| *byte == 0) {
            continue; // Images, fonts, and other binary files.
        }
        let relative = path
            .strip_prefix(&root)
            .expect("inside")
            .to_string_lossy()
            .replace('\\', "/");
        let mut language = registry.detect_path(&path);
        if language.as_str() == "plaintext" {
            if let Some(found) = registry.detect_content(&bytes[..bytes.len().min(256)]) {
                language = found;
            }
        }
        checked += 1;
        if language.as_str() == "plaintext" && !plain_by_nature(&relative) {
            plain.push(relative);
        }
    }
    assert!(checked > 300, "only {checked} text files found");
    assert_eq!(
        plain,
        Vec::<String>::new(),
        "files that would open as Plain Text"
    );
}

#[test]
fn the_stack_maps_to_its_languages() {
    let registry = LanguageRegistry::with_builtins();
    for (path, language) in [
        ("crates/nulis-core/src/lib.rs", "rust"),
        ("Cargo.toml", "toml"),
        ("Cargo.lock", "toml"),
        ("rustfmt.toml", "toml"),
        ("apps/desktop/src/App.tsx", "tsx"),
        ("apps/desktop/src/contracts.ts", "typescript"),
        ("apps/desktop/vite.config.ts", "typescript"),
        ("apps/desktop/eslint.config.js", "javascript"),
        ("package.json", "json"),
        ("apps/desktop/src-tauri/tauri.conf.json", "json"),
        ("apps/desktop/src/styles.css", "css"),
        ("apps/desktop/index.html", "html"),
        (".github/workflows/ci.yml", "yaml"),
        ("docs/USER_GUIDE.md", "markdown"),
        ("docs/settings.schema.json", "json"),
        ("apps/desktop/src-tauri/icons/icon.svg", "xml"),
        ("apps/desktop/src-tauri/Info.plist", "plist"),
        ("Casks/nulis.rb", "ruby"),
        (".gitignore", "ignore"),
    ] {
        assert_eq!(
            registry.detect_path(Path::new(path)).as_str(),
            language,
            "{path}"
        );
    }
}
