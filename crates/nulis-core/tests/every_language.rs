//! Every language in the picker goes through the editing features the
//! editor uses: viewport, Enter, Tab, a typed closing brace, Backspace,
//! Toggle Comment, and completion. Nothing may fail except Toggle Comment
//! in a language without comments, and the text must stay readable.

use nulis_core::{CommandContext, DocumentId, NulisCore, ViewportRequest};
use serde_json::{json, Value};

const SAMPLE: &str = "start {\n  value(1) <a> \"text\" -- # // x\n  ";

fn context(id: &str) -> CommandContext {
    CommandContext {
        window_id: None,
        document_id: Some(id.into()),
    }
}

fn text(core: &NulisCore, id: &str) -> String {
    core.markdown_preview_source(&DocumentId(id.into()))
        .expect("text")
        .text
}

/// A new document holding `SAMPLE` in `language`, with the caret at its end.
fn open_sample(core: &NulisCore, language: &str) -> (String, String) {
    let created = core
        .execute_command(
            "file.new",
            CommandContext::default(),
            json!({ "name": "sample", "initialText": SAMPLE }),
        )
        .expect("create");
    let id = created["documentId"].as_str().expect("id").to_owned();
    let summary = core
        .execute_command(
            "language.set",
            context(&id),
            json!({ "languageId": language }),
        )
        .unwrap_or_else(|error| panic!("{language}: language.set: {error}"));
    assert_eq!(summary["languageId"], language);
    let end = SAMPLE.len();
    core.execute_command(
        "editor.setSelections",
        context(&id),
        json!({ "selections": [{ "anchorByte": end.to_string(), "activeByte": end.to_string() }] }),
    )
    .expect("caret");
    let revision = summary["revision"].as_str().expect("revision").to_owned();
    (id, revision)
}

#[test]
fn every_language_supports_the_editing_features() {
    let root = tempfile::tempdir().expect("application data");
    let core = NulisCore::with_session_root(root.path().into());
    let languages = core
        .execute_command("language.list", CommandContext::default(), Value::Null)
        .expect("languages");
    let languages = languages.as_array().expect("array");
    assert!(languages.len() >= 44, "{}", languages.len());
    for language in languages {
        let language = language["id"].as_str().expect("id");
        let (id, mut revision) = open_sample(&core, language);
        let mut run = |command: &str, arguments: Value| -> Result<Value, String> {
            let mut arguments = arguments;
            arguments["expectedRevision"] = json!(revision);
            let result = core
                .execute_command(command, context(&id), arguments)
                .map_err(|error| error.to_string())?;
            let summary = result.get("document").unwrap_or(&result);
            if let Some(next) = summary.get("revision").and_then(Value::as_str) {
                revision = next.to_owned();
            }
            Ok(result)
        };
        for (command, arguments) in [
            ("editor.newline", json!({})),
            ("editor.tab", json!({})),
            (
                "editor.insertText",
                json!({ "text": "}", "overwrite": false, "typed": true }),
            ),
            ("editor.deleteBackward", json!({})),
            ("editor.newline", json!({})),
            (
                "editor.insertText",
                json!({ "text": "st", "overwrite": false }),
            ),
        ] {
            run(command, arguments)
                .unwrap_or_else(|error| panic!("{language}: {command}: {error}"));
        }
        match run(
            "editor.lineOperation",
            json!({ "operation": "toggleComment" }),
        ) {
            Ok(_) => {}
            Err(error) => assert!(
                error.to_lowercase().contains("comment"),
                "{language}: toggle comment: {error}"
            ),
        }
        let list = run(
            "completion.request",
            json!({ "requestId": 1, "explicit": true, "minPrefix": 2 }),
        )
        .unwrap_or_else(|error| panic!("{language}: completion: {error}"));
        if let Some(item) = list["items"].as_array().and_then(|items| items.first()) {
            run(
                "completion.accept",
                json!({
                    "replaceStart": list["replaceStart"],
                    "replaceEnd": list["replaceEnd"],
                    "text": item["insert"],
                    "snippet": item.get("snippet").cloned().unwrap_or(json!(false)),
                }),
            )
            .unwrap_or_else(|error| panic!("{language}: accept: {error}"));
        }
        let view = core
            .viewport(&ViewportRequest {
                document_id: DocumentId(id.clone()),
                start_byte: 0,
                start_line: 1,
                line_continuation: false,
                max_bytes: 64 * 1024,
            })
            .unwrap_or_else(|error| panic!("{language}: viewport: {error}"));
        assert_eq!(view.syntax.language_id, language);
        assert!(text(&core, &id).starts_with("start {"), "{language}");
    }
}
