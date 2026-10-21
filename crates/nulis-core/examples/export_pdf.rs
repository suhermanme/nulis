//! Exports a file to PDF through the core, as File ▸ Export to PDF does.
//!
//! `cargo run -p nulis-core --example export_pdf -- <input> <output.pdf> [setup JSON]`

use nulis_core::{CommandContext, NulisCore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let (Some(input), Some(output)) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: export_pdf <input> <output.pdf> [setup JSON]");
        std::process::exit(2);
    };
    let setup: serde_json::Value = match arguments.next() {
        Some(json) => serde_json::from_str(&json)?,
        None => serde_json::json!({}),
    };
    let data = tempfile::tempdir()?;
    let core = NulisCore::with_session_root(data.path().into());
    let opened = core.execute_command(
        "file.open",
        CommandContext::default(),
        serde_json::json!({ "path": std::fs::canonicalize(input)?.display().to_string() }),
    )?;
    let context = CommandContext {
        window_id: Some("main".into()),
        document_id: opened["documentId"].as_str().map(Into::into),
    };
    let output = std::path::absolute(output)?;
    let result = core.execute_command(
        "file.exportPdfTo",
        context,
        serde_json::json!({
            "expectedRevision": opened["revision"],
            "path": output.display().to_string(),
            "setup": setup,
            "date": "21 October 2026",
        }),
    )?;
    println!("{result}");
    Ok(())
}
