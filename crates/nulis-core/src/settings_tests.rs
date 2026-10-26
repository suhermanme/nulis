use super::{CommandContext, NulisCore};
use serde_json::{json, Value};

fn language_of_new(core: &NulisCore, name: &str) -> String {
    core.execute_command(
        "file.new",
        CommandContext::default(),
        json!({ "name": name, "initialText": "" }),
    )
    .expect("create document")["languageId"]
        .as_str()
        .expect("language")
        .to_owned()
}

fn state(core: &NulisCore) -> Value {
    core.execute_command("settings.state", CommandContext::default(), Value::Null)
        .expect("settings state")
}

#[test]
fn file_associations_in_settings_json_apply_at_start() {
    let root = tempfile::tempdir().expect("application data");
    std::fs::write(
        root.path().join("settings.json"),
        r#"{"schemaVersion":1,"values":{
                "files":{"associations":{"*.h":"cpp","*.foo":"no-such","a/b":"rust"}},
                "editor":{"tabSize":2},
                "[python]":{"editor":{"tabSize":8}}
            }}"#,
    )
    .expect("write settings");
    let core = NulisCore::with_session_root(root.path().into());
    assert_eq!(language_of_new(&core, "x.h"), "cpp");
    assert_eq!(language_of_new(&core, "x.c"), "c");
    let report = state(&core);
    assert_eq!(report["rejectedAssociations"], json!(["*.foo", "a/b"]));
    assert_eq!(report["loadError"], Value::Null);
    assert_eq!(core.effective_settings(None)["editor"]["tabSize"], 2);
    assert_eq!(
        core.effective_settings(Some("python"))["editor"]["tabSize"],
        8
    );
    // Defaults still fill what the file leaves out.
    assert_eq!(core.effective_settings(None)["editor"]["wordWrap"], false);
}

#[test]
fn saving_settings_applies_them_and_a_stale_save_is_refused() {
    let root = tempfile::tempdir().expect("application data");
    let core = NulisCore::with_session_root(root.path().into());
    assert_eq!(language_of_new(&core, "x.h"), "c");
    let document = json!({ "schemaVersion": 1, "values": {
        "files": { "associations": { "*.h": "objective-c" } }
    }});
    let saved = core
        .execute_command(
            "settings.saveChecked",
            CommandContext::default(),
            json!({ "expectedRevision": 0, "document": document }),
        )
        .expect("save");
    assert_eq!(saved["revision"], 1);
    assert_eq!(language_of_new(&core, "x.h"), "objective-c");
    assert!(root.path().join("settings.json").is_file());

    let stale = core.execute_command(
        "settings.saveChecked",
        CommandContext::default(),
        json!({ "expectedRevision": 0, "document": { "schemaVersion": 1, "values": {} } }),
    );
    assert!(stale.is_err());
    assert_eq!(language_of_new(&core, "x.h"), "objective-c");

    // Removing the association restores the built-in mapping, and a new
    // core reads the saved file.
    core.execute_command(
        "settings.saveChecked",
        CommandContext::default(),
        json!({ "expectedRevision": 1, "document": { "schemaVersion": 1, "values": {} } }),
    )
    .expect("save");
    assert_eq!(language_of_new(&core, "x.h"), "c");
    let invalid = core.execute_command(
        "settings.saveChecked",
        CommandContext::default(),
        json!({ "expectedRevision": 2, "document": { "schemaVersion": 1, "values": {
            "files": { "associations": ["*.h"] }
        }}}),
    );
    assert!(invalid.is_err());
    assert_eq!(state(&core)["revision"], 2);
}

#[test]
fn capabilities_can_be_switched_off_per_language_and_open_documents_follow() {
    let root = tempfile::tempdir().expect("application data");
    std::fs::write(
        root.path().join("settings.json"),
        r#"{"schemaVersion":1,"values":{
                "[json]":{"disabledCapabilities":["format","compact","unknownKind"]},
                "[markdown]":{"disabledCapabilities":["preview"]}
            }}"#,
    )
    .expect("write settings");
    let core = NulisCore::with_session_root(root.path().into());
    let json = core
        .execute_command(
            "file.new",
            CommandContext::default(),
            json!({ "name": "a.json", "initialText": "{\"a\":1}" }),
        )
        .expect("create");
    assert_eq!(
        json["capabilities"],
        json!({ "validate": "JSON", "completion": "auto" })
    );
    let id = json["documentId"].as_str().expect("id").to_owned();
    let context = CommandContext {
        window_id: None,
        document_id: Some(id.clone()),
    };
    let refused = core
        .execute_command(
            "tools.beautify",
            context.clone(),
            json!({ "expectedRevision": "0" }),
        )
        .expect_err("beautify is off");
    assert!(refused
        .to_string()
        .contains("Beautify is not available for JSON documents"));
    assert_eq!(
        core.execute_command("tools.validate", context.clone(), Value::Null)
            .expect("validate")["message"],
        "Valid JSON"
    );
    let markdown = core
        .execute_command(
            "file.new",
            CommandContext::default(),
            json!({ "name": "a.md", "initialText": "# A" }),
        )
        .expect("create");
    assert_eq!(markdown["capabilities"], json!({ "completion": "manual" }));

    // Turning everything back on reaches the open document without reopening it.
    core.execute_command(
        "settings.saveChecked",
        CommandContext::default(),
        json!({ "expectedRevision": 0, "document": { "schemaVersion": 1, "values": {} } }),
    )
    .expect("save");
    let beautified = core
        .execute_command(
            "tools.beautify",
            context,
            json!({ "expectedRevision": "0" }),
        )
        .expect("beautify");
    assert_eq!(beautified["capabilities"]["format"], "JSON");
    let summaries = core
        .execute_command("editor.summaries", CommandContext::default(), Value::Null)
        .expect("summaries");
    let markdown_now = summaries
        .as_array()
        .expect("array")
        .iter()
        .find(|summary| summary["name"] == "a.md")
        .expect("markdown document");
    assert_eq!(markdown_now["capabilities"]["preview"], "markdown");
    assert_eq!(beautified["revision"], "1");
}

#[test]
fn settings_list_each_language_s_switchable_capabilities() {
    let root = tempfile::tempdir().expect("application data");
    std::fs::write(
        root.path().join("settings.json"),
        r#"{"schemaVersion":1,"values":{"[json]":{"disabledCapabilities":["format","compact"]}}}"#,
    )
    .expect("write settings");
    let core = NulisCore::with_session_root(root.path().into());
    let listed = core
        .execute_command(
            "language.capabilities",
            CommandContext::default(),
            Value::Null,
        )
        .expect("list");
    let json_row = listed
        .as_array()
        .expect("array")
        .iter()
        .find(|row| row["id"] == "json")
        .expect("json row")
        .clone();
    assert_eq!(
        json_row["capabilities"],
        json!([
            { "kind": "diagnostics", "label": "JSON", "enabled": true },
            { "kind": "format", "label": "JSON", "enabled": false },
            { "kind": "compact", "label": "JSON", "enabled": false },
            { "kind": "indentation", "label": "Indentation", "enabled": true },
            { "kind": "completion", "label": "Words", "enabled": true }
        ])
    );
    // Plain Text has only word completion to switch.
    let plain = listed
        .as_array()
        .expect("array")
        .iter()
        .find(|row| row["id"] == "plaintext")
        .expect("plain text row");
    assert_eq!(
        plain["capabilities"],
        json!([{ "kind": "completion", "label": "Words", "enabled": true }])
    );
}

#[test]
fn a_broken_settings_file_is_reported_and_does_not_stop_the_editor() {
    let root = tempfile::tempdir().expect("application data");
    std::fs::write(root.path().join("settings.json"), b"{ broken").expect("write");
    let core = NulisCore::with_session_root(root.path().into());
    let report = state(&core);
    assert!(report["loadError"]
        .as_str()
        .is_some_and(|error| error.contains("not valid JSON")));
    assert_eq!(language_of_new(&core, "x.h"), "c");
    assert_eq!(core.effective_settings(None)["editor"]["tabSize"], 4);
}
