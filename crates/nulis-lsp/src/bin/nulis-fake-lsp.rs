//! A tiny language server for Nulis's tests; never bundled.
//!
//! It answers `initialize` (UTF-8 positions unless run with `--utf16`),
//! publishes an error for every line containing `TODO` when a document
//! opens or changes, offers two completions, and formats by removing
//! trailing spaces. `--mode=crash` exits after initializing, `--mode=hang`
//! never answers `initialize`, `--mode=slow` waits five seconds before
//! answering completion.

use nulis_lsp::{read_message, write_message};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{self, BufReader},
    thread,
    time::Duration,
};

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mode = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--mode="))
        .unwrap_or_default()
        .to_owned();
    let utf16 = arguments.iter().any(|argument| argument == "--utf16");
    let mut input = BufReader::new(io::stdin());
    let mut output = io::stdout();
    let mut documents: HashMap<String, String> = HashMap::new();
    while let Ok(Some(message)) = read_message(&mut input) {
        let method = message["method"].as_str().unwrap_or_default().to_owned();
        let id = message.get("id").cloned();
        let params = &message["params"];
        let reply = |output: &mut io::Stdout, result: Value| {
            if let Some(id) = &id {
                let _ = write_message(
                    output,
                    &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                );
            }
        };
        match method.as_str() {
            "initialize" => {
                if mode == "hang" {
                    continue;
                }
                let encoding = if utf16 { "utf-16" } else { "utf-8" };
                reply(
                    &mut output,
                    json!({ "capabilities": {
                        "positionEncoding": encoding,
                        "textDocumentSync": 1,
                        "completionProvider": {},
                        "documentFormattingProvider": true
                    }}),
                );
            }
            "initialized" => {
                if mode == "crash" {
                    std::process::exit(3);
                }
            }
            "textDocument/didOpen" | "textDocument/didChange" => {
                let uri = params["textDocument"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let text = if method.ends_with("didOpen") {
                    params["textDocument"]["text"].as_str()
                } else {
                    params["contentChanges"][0]["text"].as_str()
                }
                .unwrap_or_default()
                .to_owned();
                let diagnostics = todo_diagnostics(&text);
                let _ = write_message(
                    &mut output,
                    &json!({ "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                        "params": { "uri": uri, "diagnostics": diagnostics } }),
                );
                documents.insert(uri, text);
            }
            "textDocument/didClose" => {
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    documents.remove(uri);
                }
            }
            "textDocument/completion" => {
                if mode == "slow" {
                    thread::sleep(Duration::from_secs(5));
                }
                reply(
                    &mut output,
                    json!({ "isIncomplete": false, "items": [
                        { "label": "fakeAlpha", "kind": 3, "detail": "fn fakeAlpha()" },
                        { "label": "fakeBeta", "kind": 6, "insertText": "fakeBeta($1)", "insertTextFormat": 2 }
                    ]}),
                );
            }
            "textDocument/formatting" => {
                let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
                let text = documents.get(uri).cloned().unwrap_or_default();
                reply(&mut output, trimmed_edits(&text));
            }
            "shutdown" => reply(&mut output, Value::Null),
            "exit" => return,
            _ => {
                if id.is_some() {
                    reply(&mut output, Value::Null);
                }
            }
        }
    }
}

/// An error for every line that holds `TODO`.
fn todo_diagnostics(text: &str) -> Vec<Value> {
    text.lines()
        .enumerate()
        .filter_map(|(line, content)| {
            let column = content.find("TODO")?;
            Some(json!({
                "range": {
                    "start": { "line": line, "character": column },
                    "end": { "line": line, "character": column + 4 }
                },
                "severity": 1,
                "source": "fake",
                "message": "TODO left in the code"
            }))
        })
        .collect()
}

/// One edit replacing the whole text with its lines' trailing spaces removed.
fn trimmed_edits(text: &str) -> Value {
    let lines = text.split('\n').count();
    let formatted: Vec<&str> = text.split('\n').map(str::trim_end).collect();
    json!([{ "range": {
        "start": { "line": 0, "character": 0 },
        "end": { "line": lines, "character": 0 }
    }, "newText": formatted.join("\n") }])
}
