//! The client against the fake server built from this crate.

use nulis_lsp::{LspClient, LspError, PositionEncoding};
use serde_json::json;
use std::time::{Duration, Instant};

const SECOND: Duration = Duration::from_secs(1);

/// The fake server with its options (`--mode=crash`, `--utf16`, …).
fn fake(options: &[&str]) -> Vec<String> {
    std::iter::once(env!("CARGO_BIN_EXE_nulis-fake-lsp"))
        .chain(options.iter().copied())
        .map(str::to_owned)
        .collect()
}

fn start(options: &[&str]) -> LspClient {
    LspClient::start(&fake(options), &std::env::temp_dir(), None, 5 * SECOND).expect("start")
}

fn wait_for_diagnostics(client: &LspClient) -> serde_json::Value {
    let deadline = Instant::now() + 5 * SECOND;
    while Instant::now() < deadline {
        if let Some(found) = client
            .take_notifications()
            .into_iter()
            .find(|message| message["method"] == "textDocument/publishDiagnostics")
        {
            return found;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no diagnostics");
}

#[test]
fn it_initializes_syncs_completes_formats_and_shuts_down() {
    let client = start(&[]);
    assert_eq!(client.encoding(), PositionEncoding::Utf8);
    assert_eq!(client.capabilities()["documentFormattingProvider"], true);
    client
        .notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": "file:///x.rs", "languageId": "rust", "version": 1, "text": "ok\n  TODO fix  \n" } }),
        )
        .expect("open");
    let diagnostics = wait_for_diagnostics(&client);
    assert_eq!(
        diagnostics["params"]["diagnostics"][0]["range"]["start"],
        json!({ "line": 1, "character": 2 })
    );
    let completion = client
        .request(
            "textDocument/completion",
            json!({ "textDocument": { "uri": "file:///x.rs" }, "position": { "line": 0, "character": 1 } }),
            SECOND,
        )
        .expect("completion");
    assert_eq!(completion["items"][0]["label"], "fakeAlpha");
    let edits = client
        .request("textDocument/formatting", json!({ "textDocument": { "uri": "file:///x.rs" }, "options": { "tabSize": 4, "insertSpaces": true } }), SECOND)
        .expect("format");
    assert_eq!(edits[0]["newText"], "ok\n  TODO fix\n");
    assert!(client.is_running());
    client.shutdown(SECOND);
    assert!(!client.is_running());
}

#[test]
fn a_server_asking_for_utf16_gets_utf16() {
    let client = start(&["--utf16"]);
    assert_eq!(client.encoding(), PositionEncoding::Utf16);
}

#[test]
fn a_crashed_server_fails_its_requests_instead_of_hanging() {
    let client = start(&["--mode=crash"]);
    let started = Instant::now();
    let error = client
        .request("textDocument/completion", json!({}), 5 * SECOND)
        .expect_err("stopped");
    assert!(matches!(error, LspError::Stopped), "{error}");
    assert!(started.elapsed() < 4 * SECOND);
    assert!(!client.is_running());
}

#[test]
fn a_server_that_never_initializes_times_out_and_is_stopped() {
    let result = LspClient::start(
        &fake(&["--mode=hang"]),
        &std::env::temp_dir(),
        None,
        Duration::from_millis(300),
    );
    assert!(matches!(result, Err(LspError::Timeout(method)) if method == "initialize"));
}

#[test]
fn a_slow_answer_times_out_and_is_cancelled() {
    let client = start(&["--mode=slow"]);
    let started = Instant::now();
    let error = client
        .request(
            "textDocument/completion",
            json!({}),
            Duration::from_millis(200),
        )
        .expect_err("late");
    assert!(matches!(error, LspError::Timeout(_)), "{error}");
    assert!(started.elapsed() < SECOND);
}

#[test]
fn a_missing_program_cannot_start() {
    let result = LspClient::start(
        &["/no/such/language-server".into()],
        &std::env::temp_dir(),
        None,
        SECOND,
    );
    assert!(matches!(result, Err(LspError::Start(_))));
    assert!(matches!(
        LspClient::start(&[], &std::env::temp_dir(), None, SECOND),
        Err(LspError::Start(_))
    ));
}
