//! Golden record of Validate, Beautify, and Minify as they behaved in Nulis
//! 0.4.0 (plan 4, written before M43 moved the tools behind capability
//! providers). Every message, location, note, output text, and refusal is
//! compared with `tests/golden/tools.json`.
//!
//! The file was written once from the 0.4.0 behavior with
//! `NULIS_RECORD_GOLDEN=1 cargo test -p nulis-core --test tool_golden`.
//! Re-record only when a milestone decides to change tool behavior, and
//! list that change in the release notes.

use nulis_core::{CommandContext, DocumentId, NulisCore};
use serde_json::{json, Map, Value};
use std::path::Path;

/// A document to try: file name, and either text or raw file bytes.
struct Case {
    label: &'static str,
    name: &'static str,
    text: Option<&'static str>,
    bytes: Option<&'static [u8]>,
    /// View As before running the tools.
    language: Option<&'static str>,
}

const fn text(label: &'static str, name: &'static str, text: &'static str) -> Case {
    Case {
        label,
        name,
        text: Some(text),
        bytes: None,
        language: None,
    }
}

const CASES: &[Case] = &[
    text("json-compact", "a.json", r#"{"a":[1,2,{"b":null}],"c":"x\"y","n":1.50e10}"#),
    text("json-formatted", "a.json", "{\n  \"a\": 1\n}"),
    text("json-trailing-comma", "a.json", "{\"a\":1,}"),
    text("json-unclosed", "a.json", "{\n  \"a\": [1, 2\n}"),
    text("json-empty", "a.json", "  \n"),
    text("json-bom-crlf", "a.json", "\u{feff}{\"a\":{\"b\":[]},\r\n\"c\":{}}"),
    text("json-comment", "a.json", "{\"a\":1 // note\n}"),
    text("json-unicode", "a.json", "{\"é\":\"ü😀\",\"k\":[true,false]}"),
    Case {
        label: "json-invalid-utf8",
        name: "a.json",
        text: None,
        bytes: Some(b"{\"a\":\"\xff\"}"),
        language: None,
    },
    text("xml-nested", "a.xml", "<?xml version=\"1.0\"?><root a=\"1\"><item>text</item><empty/><!-- c --><b><c/></b></root>"),
    text("xml-preserve", "a.xml", "<r><p xml:space=\"preserve\">  <i/>  </p><q>keep <b>me</b></q></r>"),
    text("xml-mismatched", "a.xml", "<a>\n  <b></a>\n</b>"),
    text("xml-duplicate-attribute", "a.xml", "<a x=\"1\" x=\"2\"/>"),
    text("xml-two-roots", "a.xml", "<a/><b/>"),
    text("xsl-stylesheet", "a.xsl", "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\"><out/></xsl:template></xsl:stylesheet>"),
    text("yaml-plain", "a.yaml", "a:   1\nb:\n    - x\n    - y\nc: {d: 2}\n"),
    text("yaml-comments", "a.yaml", "# top\na: 1 # tail\n"),
    text("yaml-anchors", "a.yaml", "base: &b {x: 1}\nuse: *b\n"),
    text("yaml-tags", "a.yaml", "a: !Ref thing\n"),
    text("yaml-invalid", "a.yaml", "a: [1, 2\nb: 3\n"),
    text("yaml-multi-document", "a.yaml", "a: 1\n---\nb: 2\n"),
    text("plist-valid", "Info.plist", "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>A</key><string>x</string><key>B</key><array><integer>1</integer><true/></array></dict></plist>"),
    text("plist-missing-value", "Info.plist", "<plist version=\"1.0\"><dict><key>A</key></dict></plist>"),
    text("plist-wrong-root", "Info.plist", "<root/>"),
    text("plain-text", "notes.txt", "{\"a\":1}"),
    Case {
        label: "json-via-view-as",
        name: "notes.txt",
        text: Some("{\"a\":1}"),
        bytes: None,
        language: Some("json"),
    },
];

fn context(document: &str) -> CommandContext {
    CommandContext {
        window_id: None,
        document_id: Some(document.into()),
    }
}

fn outcome(result: Result<Value, nulis_core::CommandError>) -> Value {
    match result {
        Ok(value) => json!({ "ok": value }),
        Err(error) => json!({ "error": error.to_string() }),
    }
}

/// Opens the case in a fresh core; returns the core, the document ID, and
/// the summary the editor shows (after View As, when the case uses it).
fn open(case: &Case, directory: &Path) -> (NulisCore, String, Value) {
    let core = NulisCore::with_session_root(directory.join("data"));
    let summary = if let Some(bytes) = case.bytes {
        let path = directory.join(case.name);
        std::fs::write(&path, bytes).expect("write case file");
        core.execute_command(
            "file.open",
            CommandContext::default(),
            json!({ "path": path }),
        )
    } else {
        core.execute_command(
            "file.new",
            CommandContext::default(),
            json!({ "name": case.name, "initialText": case.text.unwrap_or_default() }),
        )
    }
    .expect("open case");
    let mut summary = summary;
    let id = summary["documentId"]
        .as_str()
        .expect("document ID")
        .to_owned();
    if let Some(language) = case.language {
        core.execute_command(
            "language.set",
            context(&id),
            json!({ "languageId": language }),
        )
        .expect("set language");
        summary["languageId"] = json!(language);
    }
    (core, id, summary)
}

/// Keeps what the interface reads from a summary, leaving out run-specific IDs.
///
/// M43 replaced the summary's `tool` (`"json"`, `"xml"`, …) with
/// `capabilities`; the old value is rebuilt from the Validate label so the
/// 0.4.0 record still says which tool set a document had.
fn summary_fields(value: &Value) -> Value {
    let keep = ["revision", "dirty", "languageId", "tool", "readOnly"];
    let Some(object) = value.as_object() else {
        return value.clone();
    };
    let mut fields: Map<String, Value> = keep
        .iter()
        .filter_map(|key| {
            object
                .get(*key)
                .map(|field| ((*key).to_owned(), field.clone()))
        })
        .collect();
    if let Some(label) = object
        .get("capabilities")
        .and_then(|capabilities| capabilities.get("validate"))
        .and_then(Value::as_str)
    {
        fields.insert("tool".into(), json!(label.to_ascii_lowercase()));
    }
    Value::Object(fields)
}

/// M43 removed `tool` from validation reports (the interface never read
/// it); everything else in a report is compared.
fn without_report_tool(mut value: Value) -> Value {
    if let Some(report) = value.get_mut("ok").and_then(Value::as_object_mut) {
        report.remove("tool");
    }
    value
}

fn record(case: &Case) -> Value {
    let mut result = Map::new();
    let directory = tempfile::tempdir().expect("directory");
    let (core, id, opened) = open(case, directory.path());
    result.insert("opened".into(), summary_fields(&opened));
    result.insert(
        "validate".into(),
        outcome(core.execute_command("tools.validate", context(&id), Value::Null)),
    );
    for (key, command, accept_loss) in [
        ("beautify", "tools.beautify", false),
        ("beautifyAcceptingLoss", "tools.beautify", true),
        ("minify", "tools.minify", false),
    ] {
        let directory = tempfile::tempdir().expect("directory");
        let (core, id, _) = open(case, directory.path());
        let ran = core.execute_command(
            command,
            context(&id),
            json!({ "expectedRevision": "0", "acceptLoss": accept_loss }),
        );
        let text = core
            .markdown_preview_source(&DocumentId(id.clone()))
            .map(|source| source.text)
            .unwrap_or_default();
        result.insert(
            key.into(),
            match ran {
                Ok(summary) => json!({ "ok": summary_fields(&summary), "text": text }),
                Err(error) => json!({ "error": error.to_string(), "text": text }),
            },
        );
    }
    Value::Object(result)
}

#[test]
fn tools_behave_as_in_0_4_0() {
    let golden_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/tools.json");
    let actual: Map<String, Value> = CASES
        .iter()
        .map(|case| (case.label.to_owned(), record(case)))
        .collect();
    let actual = Value::Object(actual);
    if std::env::var_os("NULIS_RECORD_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_path.parent().expect("parent")).expect("golden directory");
        let mut text = serde_json::to_string_pretty(&actual).expect("serialize");
        text.push('\n');
        std::fs::write(&golden_path, text).expect("write golden");
        return;
    }
    let expected: Value =
        serde_json::from_slice(&std::fs::read(&golden_path).expect("golden file"))
            .expect("golden JSON");
    for (label, expected_case) in expected.as_object().expect("object") {
        let mut expected_case = expected_case.clone();
        let mut actual_case = actual[label].clone();
        for case in [&mut expected_case, &mut actual_case] {
            case["validate"] = without_report_tool(case["validate"].take());
        }
        assert_eq!(actual_case, expected_case, "{label}");
    }
    assert_eq!(
        actual.as_object().map(Map::len),
        expected.as_object().map(Map::len),
        "case count"
    );
}
