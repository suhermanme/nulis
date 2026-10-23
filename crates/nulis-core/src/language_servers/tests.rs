use super::{apply_edits, first_choices};
use crate::{CommandContext, NulisCore};
use nulis_lsp::PositionEncoding;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

/// The fake server from `nulis-lsp`, built with the Cargo running these
/// tests (nothing to do when it is up to date).
fn fake_server() -> PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = std::process::Command::new(cargo)
            .args(["build", "-q", "-p", "nulis-lsp", "--bin", "nulis-fake-lsp"])
            .status()
            .expect("cargo");
        assert!(status.success(), "building the fake language server failed");
        let target = std::env::current_exe()
            .expect("test binary")
            .parent()
            .and_then(Path::parent)
            .expect("target directory")
            .to_path_buf();
        target.join(format!("nulis-fake-lsp{}", std::env::consts::EXE_SUFFIX))
    })
    .clone()
}

fn context(id: &str) -> CommandContext {
    CommandContext {
        window_id: None,
        document_id: Some(id.into()),
    }
}

/// A core whose settings configure the fake server for Rust, with options.
fn core_with_server(root: &Path, options: &[&str]) -> NulisCore {
    let mut command = vec![fake_server().to_string_lossy().into_owned()];
    command.extend(options.iter().map(|option| (*option).to_owned()));
    std::fs::write(
        root.join("settings.json"),
        json!({ "schemaVersion": 1, "values": {
            "lsp": { "servers": { "fake": { "command": command, "languages": ["rust"] } } }
        }})
        .to_string(),
    )
    .expect("settings");
    NulisCore::with_session_root(root.to_path_buf())
}

fn open(core: &NulisCore, folder: &Path, name: &str, text: &str) -> Value {
    let path = folder.join(name);
    std::fs::write(&path, text).expect("file");
    core.execute_command(
        "file.open",
        CommandContext::default(),
        json!({ "path": path }),
    )
    .expect("open")
}

fn problems(core: &NulisCore, id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let found = core
            .execute_command("lsp.diagnostics", context(id), Value::Null)
            .expect("diagnostics");
        if found["problems"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
            || Instant::now() > deadline
        {
            return found;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn nothing_runs_until_the_user_approves_the_exact_command() {
    let data = tempfile::tempdir().expect("data");
    let project = tempfile::tempdir().expect("project");
    let core = core_with_server(data.path(), &[]);
    let state = core
        .execute_command("lsp.state", CommandContext::default(), Value::Null)
        .expect("state");
    assert_eq!(state["servers"][0]["id"], "fake");
    assert_eq!(state["servers"][0]["approved"], false);
    let document = open(&core, project.path(), "main.rs", "fn main() {}\n// TODO\n");
    assert!(document["capabilities"].get("languageServer").is_none());
    let id = document["documentId"].as_str().expect("id");
    let found = core
        .execute_command("lsp.diagnostics", context(id), Value::Null)
        .expect("diagnostics");
    assert_eq!(found["server"], Value::Null);
    assert_eq!(found["problems"], json!([]));
}

#[test]
fn an_approved_server_reports_problems_completes_and_formats() {
    let data = tempfile::tempdir().expect("data");
    let project = tempfile::tempdir().expect("project");
    std::fs::write(project.path().join("Cargo.toml"), "[package]\n").expect("marker");
    std::fs::create_dir(project.path().join("src")).expect("src");
    let core = core_with_server(data.path(), &[]);
    core.execute_command(
        "lsp.approve",
        CommandContext::default(),
        json!({ "id": "fake" }),
    )
    .expect("approve");
    let document = open(
        &core,
        &project.path().join("src"),
        "main.rs",
        "fn main() {}  \n// TODO: é\n",
    );
    let id = document["documentId"].as_str().expect("id").to_owned();
    // Rust has no built-in Beautify, so the server's formatting stands in.
    assert_eq!(document["capabilities"]["languageServer"], "fake");
    assert_eq!(document["capabilities"]["format"], "fake");

    let found = problems(&core, &id);
    assert_eq!(found["server"], "fake");
    let problem = &found["problems"][0];
    assert_eq!(problem["line"], 2);
    assert_eq!(problem["severity"], "error");
    assert_eq!(problem["startByte"], "18");
    assert_eq!(problem["endByte"], "22");
    let state = core
        .execute_command("lsp.state", CommandContext::default(), Value::Null)
        .expect("state");
    assert_eq!(
        state["servers"][0]["running"][0],
        json!(project.path().to_string_lossy())
    );

    let list = core
        .execute_command(
            "completion.request",
            context(&id),
            json!({ "expectedRevision": "0", "requestId": 1, "explicit": true }),
        )
        .expect("completion");
    let labels: Vec<&str> = list["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(
        labels.contains(&"fakeAlpha") && labels.contains(&"main"),
        "{labels:?}"
    );
    let beta = list["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["label"] == "fakeBeta")
        .expect("beta");
    assert_eq!(beta["snippet"], true);
    assert_eq!(beta["kind"], "symbol");

    let formatted = core
        .execute_command(
            "tools.beautify",
            context(&id),
            json!({ "expectedRevision": "0" }),
        )
        .expect("format");
    assert_eq!(formatted["revision"], "1");
    assert_eq!(
        core.markdown_preview_source(&crate::DocumentId(id.clone()))
            .expect("text")
            .text,
        "fn main() {}\n// TODO: é\n"
    );
}

#[test]
fn changing_the_command_revokes_approval_and_closing_the_last_document_stops_the_server() {
    let data = tempfile::tempdir().expect("data");
    let project = tempfile::tempdir().expect("project");
    let core = core_with_server(data.path(), &[]);
    core.execute_command(
        "lsp.approve",
        CommandContext::default(),
        json!({ "id": "fake" }),
    )
    .expect("approve");
    let document = open(&core, project.path(), "a.rs", "// TODO\n");
    let id = document["documentId"].as_str().expect("id").to_owned();
    assert_eq!(problems(&core, &id)["server"], "fake");
    core.execute_command("file.close", context(&id), json!({ "discard": true }))
        .expect("close");
    let other = open(&core, project.path(), "notes.txt", "text\n");
    let other_id = other["documentId"].as_str().expect("id");
    core.execute_command("lsp.diagnostics", context(other_id), Value::Null)
        .expect("diagnostics");
    let state = core
        .execute_command("lsp.state", CommandContext::default(), Value::Null)
        .expect("state");
    assert_eq!(state["servers"][0]["running"], json!([]));

    // A new command in settings.json is not approved.
    let mut command = vec![fake_server().to_string_lossy().into_owned()];
    command.push("--utf16".into());
    let saved = core
        .execute_command("settings.state", CommandContext::default(), Value::Null)
        .expect("settings");
    core.execute_command(
        "settings.saveChecked",
        CommandContext::default(),
        json!({ "expectedRevision": saved["revision"], "document": { "schemaVersion": 1, "values": {
            "lsp": { "servers": { "fake": { "command": command, "languages": ["rust"] } } }
        }}}),
    )
    .expect("save");
    let state = core
        .execute_command("lsp.state", CommandContext::default(), Value::Null)
        .expect("state");
    assert_eq!(state["servers"][0]["approved"], false);
    let again = open(&core, project.path(), "b.rs", "// TODO\n");
    assert!(again["capabilities"].get("languageServer").is_none());
}

#[test]
fn a_crashing_server_is_set_aside_and_untitled_documents_never_reach_one() {
    let data = tempfile::tempdir().expect("data");
    let project = tempfile::tempdir().expect("project");
    let core = core_with_server(data.path(), &["--mode=hang"]);
    core.execute_command(
        "lsp.approve",
        CommandContext::default(),
        json!({ "id": "fake" }),
    )
    .expect("approve");
    let untitled = core
        .execute_command(
            "file.new",
            CommandContext::default(),
            json!({ "name": "scratch.rs", "initialText": "// TODO\n" }),
        )
        .expect("untitled");
    let found = core
        .execute_command(
            "lsp.diagnostics",
            context(untitled["documentId"].as_str().expect("id")),
            Value::Null,
        )
        .expect("diagnostics");
    assert_eq!(found["server"], Value::Null);

    // A server that never finishes starting fails; after three failures it
    // is set aside until restarted, and the document stays fully usable.
    let document = open(&core, project.path(), "a.rs", "// TODO\n");
    let id = document["documentId"].as_str().expect("id").to_owned();
    for attempt in 1..=3 {
        core.documents
            .servers()
            .for_tests(Duration::from_millis(200));
        let result = core.execute_command("lsp.diagnostics", context(&id), Value::Null);
        assert!(result.is_err(), "attempt {attempt}");
    }
    core.documents
        .servers()
        .for_tests(Duration::from_millis(200));
    let quiet = core
        .execute_command("lsp.diagnostics", context(&id), Value::Null)
        .expect("set aside, not failing");
    assert_eq!(quiet["server"], Value::Null);
    let state = core
        .execute_command("lsp.state", CommandContext::default(), Value::Null)
        .expect("state");
    assert_eq!(state["servers"][0]["stopped"], true);
    assert!(state["servers"][0]["error"]
        .as_str()
        .is_some_and(|error| error.contains("initialize")));
    let typed = core
        .execute_command(
            "editor.insertText",
            context(&id),
            json!({ "expectedRevision": "0", "text": "x" }),
        )
        .expect("typing still works");
    assert_eq!(typed["revision"], "1");
    // Restart clears it.
    core.execute_command(
        "lsp.restart",
        CommandContext::default(),
        json!({ "id": "fake" }),
    )
    .expect("restart");
    let state = core
        .execute_command("lsp.state", CommandContext::default(), Value::Null)
        .expect("state");
    assert_eq!(state["servers"][0]["stopped"], false);
}

#[test]
fn edits_apply_from_the_end_and_choices_become_placeholders() {
    let edits = json!([
        { "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } }, "newText": "X" },
        { "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 2 } }, "newText": "yy" }
    ]);
    assert_eq!(
        apply_edits("ab\ncd\n", &edits, PositionEncoding::Utf8),
        "Xb\nyy\n"
    );
    assert_eq!(first_choices("x(${1|a,b|}) $0"), "x(${1:a}) $0");
    assert_eq!(first_choices("${2:keep} ${x}"), "${2:keep} ${x}");
}
