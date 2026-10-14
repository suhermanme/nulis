//! Converts a pinned Base46 source snapshot to Nulis's bundled theme catalog.

use nulis_theme::import_nvchad_theme;
use std::{env, error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args().skip(1);
    let source_directory = arguments
        .next()
        .ok_or("usage: import_base46 SOURCE_DIR OUTPUT_JSON")?;
    let output = arguments
        .next()
        .ok_or("usage: import_base46 SOURCE_DIR OUTPUT_JSON")?;
    if arguments.next().is_some() {
        return Err("usage: import_base46 SOURCE_DIR OUTPUT_JSON".into());
    }

    let mut paths = fs::read_dir(&source_directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    let mut themes = Vec::new();
    for path in paths {
        if path.extension().is_none_or(|extension| extension != "lua") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or("invalid filename")?;
        let source = fs::read_to_string(&path)?;
        let theme = import_nvchad_theme(&source, name)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        themes.push(theme);
    }
    if themes.is_empty() {
        return Err("no Base46 themes found".into());
    }
    let bytes = serde_json::to_vec_pretty(&themes)?;
    fs::write(Path::new(&output), bytes)?;
    println!("Converted {} Base46 themes", themes.len());
    Ok(())
}
