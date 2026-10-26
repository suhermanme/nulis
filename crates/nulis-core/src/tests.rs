use super::{CommandContext, NulisCore, APPLICATION_ID, IPC_PROTOCOL_VERSION};
use serde_json::Value;
use std::{sync::Arc, thread, time::Duration};

#[test]
fn exposes_centralized_identity_and_protocol() {
    let info = NulisCore::default().application_info();
    assert_eq!(info.application_id, APPLICATION_ID);
    assert_eq!(info.ipc_protocol_version, IPC_PROTOCOL_VERSION);
}

#[test]
fn built_in_command_executes_headlessly() {
    let result = NulisCore::default()
        .execute_command("app.about", CommandContext::default(), Value::Null)
        .expect("command executes");
    assert_eq!(result["productName"], "Nulis");
}

#[test]
fn export_pdf_reads_every_window_and_refuses_unsafe_destinations() {
    let application_data = tempfile::tempdir().expect("application data");
    let folder = tempfile::tempdir().expect("folder");
    let core = NulisCore::with_session_root(application_data.path().into());
    let open = |name: &str, text: &str| {
        let path = folder.path().join(name);
        std::fs::write(&path, text).expect("write source");
        let opened = core
            .execute_command(
                "file.open",
                CommandContext::default(),
                serde_json::json!({ "path": path.display().to_string() }),
            )
            .expect("open");
        (
            path,
            CommandContext {
                window_id: Some("main".into()),
                document_id: Some(opened["documentId"].as_str().expect("document ID").into()),
            },
        )
    };
    let export = |context: &CommandContext, arguments: serde_json::Value| {
        core.execute_command("file.exportPdfTo", context.clone(), arguments)
    };

    // 40,000 short lines (about 430 KB) take two reading windows; the
    // default A4 page holds 61 rows.
    let mut text = String::new();
    for line in 1..=40_000 {
        text.push_str("line ");
        text.push_str(&line.to_string());
        text.push('\n');
    }
    let (source, context) = open("lines.txt", &text);
    let pdf = folder.path().join("lines.pdf");
    let result = export(
        &context,
        serde_json::json!({ "expectedRevision": "0", "path": pdf.display().to_string(), "date": "today" }),
    )
    .expect("export");
    // 40,001 lines: the newline at the end starts an empty last line.
    assert_eq!(result["pages"], "656");
    assert_eq!(result["missingGlyphs"], "0");
    let bytes = std::fs::read(&pdf).expect("read PDF");
    assert!(bytes.starts_with(b"%PDF-1.7"));
    assert_eq!(result["bytes"], bytes.len().to_string());
    let leftovers = std::fs::read_dir(folder.path())
        .expect("list")
        .filter(|entry| {
            entry.as_ref().is_ok_and(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".nulis-export-")
            })
        })
        .count();
    assert_eq!(leftovers, 0, "the temporary file is moved into place");

    // Refusals: the document's own file, a stale revision, a relative path.
    for (path, revision) in [
        (source.display().to_string(), "0"),
        (pdf.display().to_string(), "7"),
        ("relative.pdf".to_owned(), "0"),
    ] {
        assert!(
            export(
                &context,
                serde_json::json!({ "expectedRevision": revision, "path": path })
            )
            .is_err(),
            "{path} at {revision}"
        );
    }
    assert_eq!(std::fs::read_to_string(&source).expect("source"), text);

    // One 700 KiB line with no line break is read in three windows that
    // continue each other: 716,800 characters over 90 columns is 7,965
    // rows, 131 pages, with no page count in the footer.
    let (_, long_context) = open("long.txt", &"x".repeat(700 * 1024));
    let result = export(
        &long_context,
        serde_json::json!({
            "expectedRevision": "0",
            "path": folder.path().join("long.pdf").display().to_string(),
            "setup": { "syntaxHighlighting": false, "footer": { "center": "{page}" } },
        }),
    )
    .expect("export long line");
    assert_eq!(result["pages"], "131");
}

/// Opens a document of `rows` numbered lines in `folder`.
fn open_rows(core: &NulisCore, folder: &std::path::Path, rows: u32) -> CommandContext {
    let source = folder.join("rows.txt");
    let mut text = String::new();
    for line in 1..=rows {
        text.push_str("row ");
        text.push_str(&line.to_string());
        text.push('\n');
    }
    std::fs::write(&source, &text).expect("write source");
    let opened = core
        .execute_command(
            "file.open",
            CommandContext::default(),
            serde_json::json!({ "path": source.display().to_string() }),
        )
        .expect("open");
    CommandContext {
        window_id: Some("main".into()),
        document_id: Some(opened["documentId"].as_str().expect("document ID").into()),
    }
}

/// Cancels at its first progress report.
#[derive(Default)]
struct CancelAtOnce(std::sync::atomic::AtomicBool);

impl crate::editor::ExportObserver for CancelAtOnce {
    fn cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn report(&self, _progress: crate::editor::ExportProgress) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[test]
fn export_jobs_ask_before_very_large_pdfs() {
    let application_data = tempfile::tempdir().expect("application data");
    let folder = tempfile::tempdir().expect("folder");
    let core = NulisCore::with_session_root(application_data.path().into());
    let context = open_rows(&core, folder.path(), 70_000);
    let pdf = folder.path().join("big.pdf");
    let run = |confirmed: bool| {
        let started = core
            .execute_command(
                "file.exportPdfStart",
                context.clone(),
                serde_json::json!({
                    "expectedRevision": "0",
                    "path": pdf.display().to_string(),
                    "confirmed": confirmed,
                    "setup": { "syntaxHighlighting": false },
                }),
            )
            .expect("start");
        let job = serde_json::json!({ "jobId": started["jobId"] });
        for _ in 0..6_000 {
            let report = core
                .execute_command(
                    "file.exportPdfProgress",
                    CommandContext::default(),
                    job.clone(),
                )
                .expect("progress");
            if report["done"] == true {
                return report;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the export did not finish");
    };
    // 70,001 lines at 61 rows a page: at least 1,148 pages.
    let asked = run(false);
    assert_eq!(asked["needsConfirmation"], "1148");
    assert!(!pdf.exists(), "nothing is written before the user agrees");
    let done = run(true);
    assert_eq!(done["result"]["pages"], "1148");
    assert_eq!(done["progress"]["phase"], "writing");
    assert_eq!(done["progress"]["done"], done["progress"]["total"]);
    assert!(pdf.exists());
}

/// Starts a Print and waits for its report.
fn run_print(core: &NulisCore, context: &CommandContext, confirmed: bool) -> serde_json::Value {
    let started = core
        .execute_command(
            "file.printStart",
            context.clone(),
            serde_json::json!({
                "expectedRevision": "0",
                "confirmed": confirmed,
                "date": "today",
                "setup": { "syntaxHighlighting": false },
            }),
        )
        .expect("start");
    let job = serde_json::json!({ "jobId": started["jobId"] });
    for _ in 0..6_000 {
        let report = core
            .execute_command(
                "file.exportPdfProgress",
                CommandContext::default(),
                job.clone(),
            )
            .expect("progress");
        if report["done"] == true {
            return report;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the print layout did not finish");
}

#[test]
fn print_lays_out_the_same_pages_as_the_pdf_and_hands_them_out_in_batches() {
    let application_data = tempfile::tempdir().expect("application data");
    let folder = tempfile::tempdir().expect("folder");
    let core = NulisCore::with_session_root(application_data.path().into());
    let context = open_rows(&core, folder.path(), 200);
    let pdf = folder.path().join("rows.pdf");
    let exported = core
        .execute_command(
            "file.exportPdfTo",
            context.clone(),
            serde_json::json!({
                "expectedRevision": "0",
                "path": pdf.display().to_string(),
                "date": "today",
                "setup": { "syntaxHighlighting": false },
            }),
        )
        .expect("export");
    let report = run_print(&core, &context, false);
    assert_eq!(report["printedPages"], exported["pages"]);
    assert_eq!(report["printedPages"], "4");

    let read = |start: usize, count: usize| {
        core.execute_command(
            "print.pages",
            CommandContext::default(),
            serde_json::json!({ "start": start, "count": count }),
        )
    };
    let batch = read(1, 2).expect("pages");
    assert_eq!(batch["total"], 4);
    assert_eq!(batch["start"], 1);
    assert_eq!(batch["title"], "rows.txt");
    assert!((batch["frame"]["widthMm"].as_f64().expect("width") - 210.0).abs() < 0.01);
    let pages = batch["pages"].as_array().expect("pages");
    assert_eq!(pages.len(), 2);
    let second: serde_json::Value =
        serde_json::from_str(pages[0].as_str().expect("page JSON")).expect("page");
    assert_eq!(second["number"], 2);
    // 61 rows a page: page 2 starts with line 62.
    assert_eq!(second["rows"][0][0][3], " 62");
    assert_eq!(second["rows"][0][1][3], "row 62");
    assert_eq!(second["header"][0][3], "rows.txt");
    assert_eq!(second["footer"][0][3], "Page 2 of 4");
    // Reading past the end returns no pages.
    assert_eq!(
        read(9, 5).expect("past the end")["pages"],
        serde_json::json!([])
    );

    core.execute_command(
        "print.release",
        CommandContext::default(),
        serde_json::Value::Null,
    )
    .expect("release");
    assert!(read(0, 1).is_err(), "released pages are gone");
}

#[test]
fn the_page_setup_preview_lays_out_only_the_first_page() {
    let application_data = tempfile::tempdir().expect("application data");
    let folder = tempfile::tempdir().expect("folder");
    let core = NulisCore::with_session_root(application_data.path().into());
    let context = open_rows(&core, folder.path(), 70_000);
    let preview = core
        .execute_command(
            "file.printPreview",
            context.clone(),
            serde_json::json!({
                "expectedRevision": "0",
                "date": "today",
                "setup": { "orientation": "landscape", "fontSize": 12 },
            }),
        )
        .expect("preview");
    let frame = &preview["frame"];
    assert!((frame["widthMm"].as_f64().expect("width") - 297.0).abs() < 0.01);
    assert_eq!(frame["landscape"], true);
    assert_eq!(preview["page"]["number"], 1);
    assert_eq!(preview["page"]["rows"][0][1][3], "row 1");
    // The page count is estimated, not counted: no pass over 70,000 lines.
    let footer = preview["page"]["footer"][0][3].as_str().expect("footer");
    assert!(footer.starts_with("Page 1 of "), "{footer}");
    // A setup the core refuses is reported, not shown.
    assert!(core
        .execute_command(
            "file.printPreview",
            context,
            serde_json::json!({ "expectedRevision": "0", "setup": { "fontSize": 99 } }),
        )
        .is_err());
}

#[test]
fn print_asks_above_a_thousand_pages_and_refuses_above_five_thousand() {
    let application_data = tempfile::tempdir().expect("application data");
    let folder = tempfile::tempdir().expect("folder");
    let core = NulisCore::with_session_root(application_data.path().into());
    let context = open_rows(&core, folder.path(), 70_000);
    assert_eq!(
        run_print(&core, &context, false)["needsConfirmation"],
        "1148"
    );
    let confirmed = run_print(&core, &context, true);
    assert_eq!(confirmed["printedPages"], "1148");

    let context = open_rows(&core, folder.path(), 400_000);
    let refused = run_print(&core, &context, true);
    let error = refused["error"].as_str().expect("refused");
    assert!(
        error.starts_with("Could not print: this needs at least 6,558 pages"),
        "{error}"
    );
    assert!(error.contains("Print takes at most 5,000"), "{error}");
}

#[test]
fn a_comment_across_a_highlight_window_edge_keeps_its_color() {
    let application_data = tempfile::tempdir().expect("application data");
    let folder = tempfile::tempdir().expect("folder");
    let core = NulisCore::with_session_root(application_data.path().into());
    // Filler up to 300 bytes before the first window edge, then a block
    // comment that runs well past it.
    let edge = usize::try_from(crate::HIGHLIGHT_WINDOW_BYTES).expect("small");
    let mut text = String::new();
    while text.len() < edge - 300 {
        text.push_str("int x;\n");
    }
    text.push_str("/* a comment that starts before the edge\n");
    for _ in 0..20 {
        text.push_str("   still inside the comment\n");
    }
    text.push_str("*/\nint after;\n");
    let source = folder.path().join("edge.c");
    std::fs::write(&source, &text).expect("write source");
    let opened = core
        .execute_command(
            "file.open",
            CommandContext::default(),
            serde_json::json!({ "path": source.display().to_string() }),
        )
        .expect("open");
    let document_id = crate::editor::DocumentId(opened["documentId"].as_str().expect("id").into());
    let mut cursor = Some(crate::editor::ViewportCursor {
        byte_offset: 0,
        line: 1,
        line_continuation: false,
    });
    let mut windows = Vec::new();
    while let Some(at) = cursor {
        let (lines, next) = core
            .documents
            .print_window(&document_id, 0, &at, true)
            .expect("window");
        windows.push(lines);
        cursor = next;
    }
    assert!(windows.len() >= 2, "the comment crosses a window edge");
    let second = &windows[1];
    let inside = second
        .iter()
        .find(|line| line.text.contains("still inside"))
        .expect("comment line in the second window");
    assert!(
        inside
            .spans
            .iter()
            .any(|span| span.token == nulis_syntax::SyntaxToken::Comment && span.end > 6),
        "{inside:?}"
    );
    let after = second
        .iter()
        .find(|line| line.text == "int after;")
        .expect("code after the comment");
    assert!(after
        .spans
        .iter()
        .all(|span| span.token != nulis_syntax::SyntaxToken::Comment));
}

#[test]
fn a_cancelled_export_writes_nothing() {
    let application_data = tempfile::tempdir().expect("application data");
    let folder = tempfile::tempdir().expect("folder");
    let core = NulisCore::with_session_root(application_data.path().into());
    let context = open_rows(&core, folder.path(), 5_000);
    let pdf = folder.path().join("cancelled.pdf");
    let outcome = core.documents.export_pdf(
        &crate::editor::DocumentId(context.document_id.clone().expect("document")),
        &crate::editor::PdfExportRequest {
            path: pdf.clone(),
            expected_revision: 0,
            setup: nulis_print::PageSetup::default(),
            colors: nulis_print::ColorChoice::Print,
            date: String::new(),
            confirm_above: None,
            scope: crate::editor::ExportScope::Document,
        },
        &CancelAtOnce::default(),
    );
    assert_eq!(outcome, Err(crate::EditorError::ExportCancelled));
    assert!(!pdf.exists());
    let leftovers = std::fs::read_dir(folder.path())
        .expect("list")
        .filter(|entry| {
            entry.as_ref().is_ok_and(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".nulis-export-")
            })
        })
        .count();
    assert_eq!(leftovers, 0, "a cancelled export leaves no temporary file");
}

#[test]
fn file_save_commands_execute_headlessly() {
    let application_data = tempfile::tempdir().expect("application data");
    let destination = tempfile::tempdir().expect("destination");
    let path = destination.path().join("saved.txt");
    let core = NulisCore::with_session_root(application_data.path().into());
    let created = core
        .execute_command(
            "file.new",
            CommandContext::default(),
            serde_json::json!({ "name": "Untitled", "initialText": "alpha" }),
        )
        .expect("create document");
    let context = CommandContext {
        window_id: Some("main".into()),
        document_id: Some(created["documentId"].as_str().expect("document ID").into()),
    };

    let saved = core
        .execute_command(
            "file.saveAs",
            context.clone(),
            serde_json::json!({ "expectedRevision": "0", "path": path }),
        )
        .expect("save as");
    assert_eq!(saved["name"], "saved.txt");
    assert_eq!(
        std::fs::read_to_string(&path).expect("saved content"),
        "alpha"
    );

    core.execute_command(
        "editor.insertText",
        context.clone(),
        serde_json::json!({
            "expectedRevision": "0", "text": "beta", "overwrite": false
        }),
    )
    .expect("edit saved document");
    core.execute_command(
        "file.save",
        context,
        serde_json::json!({ "expectedRevision": "1" }),
    )
    .expect("save existing destination");
    assert_eq!(
        std::fs::read_to_string(path).expect("updated content"),
        "betaalpha"
    );
}

#[test]
fn recorded_macros_persist_and_resolve_current_document_revisions() {
    let application_data = tempfile::tempdir().expect("application data");
    let core = NulisCore::with_session_root(application_data.path().into());
    let created = core
        .execute_command(
            "file.new",
            CommandContext::default(),
            serde_json::json!({ "name": "Macro target", "initialText": "" }),
        )
        .expect("create document");
    let document_id = created["documentId"].as_str().expect("document ID");
    let context = CommandContext {
        window_id: Some("main".into()),
        document_id: Some(document_id.into()),
    };
    core.execute_command(
        "automation.startRecording",
        context.clone(),
        serde_json::json!({ "id": "user.insert-x", "name": "Insert X" }),
    )
    .expect("start recording");
    core.execute_command(
        "editor.insertText",
        context.clone(),
        serde_json::json!({
            "expectedRevision": "0", "text": "x", "overwrite": false
        }),
    )
    .expect("recorded insert");
    let definition = core
        .execute_command("automation.stopRecording", context.clone(), Value::Null)
        .expect("stop recording");
    assert_eq!(
        definition["actions"][0]["arguments"]["expectedRevision"],
        "$nulis.currentRevision"
    );

    core.execute_command("editor.undo", context.clone(), Value::Null)
        .expect("undo recorded edit");
    let outcome = core
        .execute_command(
            "automation.runMacro",
            context,
            serde_json::json!({ "id": "user.insert-x" }),
        )
        .expect("run macro against current revision");
    assert_eq!(outcome["actionsCompleted"], 1);
    assert_eq!(outcome["document"]["byteLen"], "1");

    let reopened = NulisCore::with_session_root(application_data.path().into());
    let listed = reopened
        .execute_command(
            "automation.listMacros",
            CommandContext::default(),
            Value::Null,
        )
        .expect("list persisted macros");
    assert_eq!(listed[0]["id"], "user.insert-x");
    assert_eq!(listed[0]["actionCount"], 1);
}

#[test]
fn scripts_require_persisted_approval_and_use_bounded_core_adapters() {
    let application_data = tempfile::tempdir().expect("application data");
    let core = NulisCore::with_session_root(application_data.path().into());
    let created = core
        .execute_command(
            "file.new",
            CommandContext::default(),
            serde_json::json!({ "name": "Script target", "initialText": "" }),
        )
        .expect("create document");
    let context = CommandContext {
        window_id: Some("main".into()),
        document_id: Some(created["documentId"].as_str().expect("document ID").into()),
    };
    core.execute_command(
        "automation.saveScript",
        context.clone(),
        serde_json::json!({
            "id": "user.edit",
            "definition": {
                "apiVersion": 1,
                "name": "Edit",
                "source": r#"nulis_read_document(); nulis_edit_document("{\"expectedRevision\":\"$nulis.currentRevision\",\"startByte\":\"0\",\"endByte\":\"0\",\"text\":\"x\"}"); nulis_show_message("done"); nulis_print("ok");"#,
                "requestedCapabilities": ["readDocument", "editDocument", "showMessage"],
                "requestedCommands": []
            }
        }),
    )
    .expect("save script");
    assert!(core
        .execute_command(
            "automation.runScript",
            context.clone(),
            serde_json::json!({ "id": "user.edit" }),
        )
        .is_err());

    core.execute_command(
        "automation.approveScript",
        context.clone(),
        serde_json::json!({
            "id": "user.edit",
            "policy": {
                "capabilities": ["readDocument", "editDocument", "showMessage"],
                "allowedCommands": [],
                "limits": {
                    "memoryBytes": 16_777_216,
                    "executionMillis": 1000,
                    "operations": 5_000_000,
                    "hostCalls": 1000,
                    "outputBytes": 65536
                }
            }
        }),
    )
    .expect("approve script");
    let outcome = core
        .execute_command(
            "automation.runScript",
            context,
            serde_json::json!({ "id": "user.edit" }),
        )
        .expect("run script");
    assert_eq!(outcome["document"]["revision"], "1");
    assert_eq!(outcome["report"]["output"], "ok\n");
    assert_eq!(outcome["messages"], serde_json::json!(["done"]));

    let reopened = NulisCore::with_session_root(application_data.path().into());
    let listed = reopened
        .execute_command(
            "automation.listScripts",
            CommandContext::default(),
            Value::Null,
        )
        .expect("list scripts");
    assert_eq!(listed[0]["id"], "user.edit");
    assert_eq!(listed[0]["approved"], true);
}

#[test]
fn scripts_use_symlink_safe_workspace_and_settings_adapters() {
    let application_data = tempfile::tempdir().expect("application data");
    let workspace = tempfile::tempdir().expect("workspace");
    let path = workspace.path().join("note.txt");
    std::fs::write(&path, "before").expect("workspace file");
    let core = NulisCore::with_session_root(application_data.path().into());
    core.execute_command(
        "workspace.open",
        CommandContext::default(),
        serde_json::json!({ "root": workspace.path() }),
    )
    .expect("open workspace");
    core.execute_command(
        "automation.saveScript",
        CommandContext::default(),
        serde_json::json!({
            "id": "user.workspace",
            "definition": {
                "apiVersion": 1,
                "name": "Workspace",
                "source": r#"nulis_read_workspace("{\"path\":\"\",\"maxEntries\":10}"); nulis_write_workspace("{\"path\":\"note.txt\",\"contents\":\"after\"}"); nulis_print(nulis_read_settings());"#,
                "requestedCapabilities": ["readWorkspace", "writeWorkspace", "readSettings"],
                "requestedCommands": []
            }
        }),
    )
    .expect("save script");
    core.execute_command(
        "automation.approveScript",
        CommandContext::default(),
        serde_json::json!({
            "id": "user.workspace",
            "policy": {
                "capabilities": ["readWorkspace", "writeWorkspace", "readSettings"],
                "allowedCommands": [],
                "limits": {
                    "memoryBytes": 16_777_216,
                    "executionMillis": 1000,
                    "operations": 5_000_000,
                    "hostCalls": 1000,
                    "outputBytes": 65536
                }
            }
        }),
    )
    .expect("approve script");
    let outcome = core
        .execute_command(
            "automation.runScript",
            CommandContext::default(),
            serde_json::json!({ "id": "user.workspace" }),
        )
        .expect("run script");
    assert_eq!(
        std::fs::read_to_string(path).expect("updated file"),
        "after"
    );
    assert!(outcome["report"]["output"]
        .as_str()
        .expect("settings output")
        .contains("workbench"));
}

#[test]
fn scripts_cannot_request_terminal_commands() {
    let core = NulisCore::default();
    let result = core.execute_command(
        "automation.saveScript",
        CommandContext::default(),
        serde_json::json!({
            "id": "user.unsafe",
            "definition": {
                "apiVersion": 1,
                "name": "Unsafe",
                "source": "",
                "requestedCapabilities": ["executeCommand"],
                "requestedCommands": ["terminal.create"]
            }
        }),
    );
    assert!(result.is_err());
}

#[test]
fn active_scripts_can_be_cancelled_through_the_command_boundary() {
    let core = Arc::new(NulisCore::default());
    core.execute_command(
        "automation.saveScript",
        CommandContext::default(),
        serde_json::json!({
            "id": "user.loop",
            "definition": {
                "apiVersion": 1,
                "name": "Loop",
                "source": "loop {}",
                "requestedCapabilities": [],
                "requestedCommands": []
            }
        }),
    )
    .expect("save script");
    core.execute_command(
        "automation.approveScript",
        CommandContext::default(),
        serde_json::json!({
            "id": "user.loop",
            "policy": {
                "capabilities": [],
                "allowedCommands": [],
                "limits": {
                    "memoryBytes": 16_777_216,
                    "executionMillis": 10_000,
                    "operations": 100_000_000,
                    "hostCalls": 1000,
                    "outputBytes": 65_536
                }
            }
        }),
    )
    .expect("approve script");
    let runner = Arc::clone(&core);
    let running = thread::spawn(move || {
        runner.execute_command(
            "automation.runScript",
            CommandContext::default(),
            serde_json::json!({ "id": "user.loop" }),
        )
    });
    let mut cancelled = false;
    for _ in 0..100 {
        let result = core
            .execute_command(
                "automation.cancelScript",
                CommandContext::default(),
                Value::Null,
            )
            .expect("cancel command");
        if result == Value::Bool(true) {
            cancelled = true;
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(cancelled, "script did not become active");
    let result = running.join().expect("script thread");
    assert!(result
        .expect_err("script should be cancelled")
        .to_string()
        .contains("cancelled"));
}

#[test]
fn recorder_excludes_non_editor_capability_commands() {
    let core = NulisCore::default();
    core.execute_command(
        "automation.startRecording",
        CommandContext::default(),
        serde_json::json!({ "id": "user.safe", "name": "Safe" }),
    )
    .expect("start recording");
    core.execute_command("app.about", CommandContext::default(), Value::Null)
        .expect("non-recordable command");
    assert!(core
        .execute_command(
            "automation.stopRecording",
            CommandContext::default(),
            Value::Null,
        )
        .is_err());
    let status = core
        .execute_command(
            "automation.recordingStatus",
            CommandContext::default(),
            Value::Null,
        )
        .expect("recording status");
    assert_eq!(status["actionCount"], 0);
    core.execute_command(
        "automation.cancelRecording",
        CommandContext::default(),
        Value::Null,
    )
    .expect("cancel recording");

    let unsafe_definition = super::MacroDefinition {
        schema_version: super::AUTOMATION_SCHEMA_VERSION,
        id: "user.unsafe".into(),
        name: "Unsafe".into(),
        actions: vec![super::RecordedCommand {
            command: "terminal.create".into(),
            arguments: serde_json::json!({}),
        }],
    };
    assert!(core
        .execute_macro(&unsafe_definition, &CommandContext::default(), || false,)
        .is_err());
}

#[test]
fn workspace_and_session_commands_share_validated_core_state() {
    let application_data = tempfile::tempdir().expect("application data");
    let workspace = tempfile::tempdir().expect("workspace");
    std::fs::create_dir(workspace.path().join("src")).expect("source directory");
    std::fs::write(workspace.path().join("src/main.txt"), "hello").expect("source file");
    let core = NulisCore::with_session_root(application_data.path().into());

    let opened = core
        .execute_command(
            "workspace.open",
            CommandContext::default(),
            serde_json::json!({ "root": workspace.path() }),
        )
        .expect("open workspace");
    assert_eq!(
        opened["name"],
        workspace
            .path()
            .file_name()
            .expect("name")
            .to_string_lossy()
            .as_ref()
    );
    let listing = core
        .execute_command(
            "workspace.list",
            CommandContext::default(),
            serde_json::json!({ "path": "", "maxEntries": 10 }),
        )
        .expect("list workspace");
    assert_eq!(listing["entries"][0]["relativePath"], "src");
    let document = core
        .execute_command(
            "workspace.openFile",
            CommandContext::default(),
            serde_json::json!({ "path": "src/main.txt" }),
        )
        .expect("open workspace file");
    assert_eq!(document["byteLen"], "5");

    let source_path = document["sourcePath"].as_str().expect("source path");
    let reference = format!("path:{source_path}");
    let mut session = serde_json::json!({
        "schemaVersion": 2,
        "workspaceRoot": opened["root"],
        "documents": [reference],
        "layouts": [{
            "id": "layout-1",
            "name": "Writing",
            "root": { "kind": "pane", "id": "pane-1", "tabs": [reference], "activeTab": reference },
            "activePane": "pane-1"
        }],
        "activeLayout": "layout-1",
        "sidebarVisible": true,
        "sidebarWidth": 300,
        "activePanel": "files",
        "viewModes": {}
    });
    session["viewModes"][&reference] = serde_json::json!("text");
    core.execute_command("session.save", CommandContext::default(), session.clone())
        .expect("save session");
    let restored = core
        .execute_command("session.load", CommandContext::default(), Value::Null)
        .expect("load session");
    assert_eq!(restored, session);
}

#[test]
fn workbench_layout_actions_are_centralized_commands() {
    let core = NulisCore::default();
    let command_ids = core
        .command_descriptors()
        .into_iter()
        .map(|command| command.id)
        .collect::<Vec<_>>();
    assert!(command_ids.contains(&"workbench.toggleSidebar".into()));
    assert!(command_ids.contains(&"workbench.toggleSplit".into()));
    assert!(command_ids.contains(&"workbench.toggleEditorMode".into()));
    let result = core
        .execute_command(
            "workbench.toggleSidebar",
            CommandContext::default(),
            Value::Null,
        )
        .expect("layout command");
    assert_eq!(result["action"], "toggleSidebar");
    for id in [
        "workspace.rename",
        "workspace.groupRight",
        "workspace.groupDown",
        "workspace.ungroup",
        "workspace.popOutPane",
        "workbench.moveTabLeft",
        "workbench.moveTabRight",
        "view.toggleLineChangeIndicators",
        "workbench.openSettings",
        "workbench.openHelp",
    ] {
        assert!(command_ids.contains(&id.into()), "{id} is registered");
    }
}

#[test]
fn document_commands_and_viewports_share_headless_state() {
    let core = NulisCore::default();
    let created = core
        .execute_command(
            "file.new",
            CommandContext::default(),
            serde_json::json!({ "name": "Test", "initialText": "one\ntwo" }),
        )
        .expect("create document");
    let document_id = created["documentId"].as_str().expect("document ID");
    let snapshot = core
        .viewport(&super::ViewportRequest {
            document_id: super::DocumentId(document_id.into()),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(snapshot.lines.len(), 2);
    assert_eq!(snapshot.lines[1].text, "two");

    let moved = core
        .execute_command(
            "editor.moveVertical",
            CommandContext {
                window_id: None,
                document_id: Some(document_id.into()),
            },
            serde_json::json!({ "direction": "down", "extend": false }),
        )
        .expect("move vertically through command boundary");
    assert_eq!(moved[0]["activeByte"], "4");
}

#[test]
fn language_commands_list_modules_and_override_detection() {
    let core = NulisCore::default();
    let created = core
        .execute_command(
            "file.new",
            CommandContext::default(),
            serde_json::json!({ "name": "data.json", "initialText": "{\"ok\":true}" }),
        )
        .expect("create JSON document");
    let document_id = created["documentId"].as_str().expect("document ID");
    let languages = core
        .execute_command("language.list", CommandContext::default(), Value::Null)
        .expect("list languages");
    assert!(languages
        .as_array()
        .expect("language array")
        .iter()
        .any(|language| language["id"] == "json"));
    let selected = core
        .execute_command(
            "language.set",
            CommandContext {
                window_id: None,
                document_id: Some(document_id.into()),
            },
            serde_json::json!({ "languageId": "plaintext" }),
        )
        .expect("set language");
    // The new summary comes back, so tools follow View As at once.
    assert_eq!(selected["languageId"], "plaintext");
    assert_eq!(selected["languageOverride"], true);
    // Plain Text has no tools, only word completion when asked for.
    assert_eq!(
        selected["capabilities"],
        serde_json::json!({ "completion": "manual" })
    );
    assert_eq!(created["capabilities"]["validate"], "JSON");
    assert_eq!(created["capabilities"]["compact"], "JSON");
}

#[test]
fn hexadecimal_commands_share_revision_checked_document_state() {
    let core = NulisCore::default();
    let created = core
        .execute_command(
            "file.new",
            CommandContext::default(),
            serde_json::json!({ "name": "bytes.bin", "initialText": "A" }),
        )
        .expect("create document");
    let document_id = created["documentId"].as_str().expect("document ID");
    let context = CommandContext {
        window_id: None,
        document_id: Some(document_id.into()),
    };
    let edit = core
        .execute_command(
            "hex.typeNibble",
            context.clone(),
            serde_json::json!({
                "expectedRevision": "0",
                "highNibble": true,
                "nibble": 15,
                "overwrite": true
            }),
        )
        .expect("hex command");
    assert_eq!(edit["document"]["revision"], "1");
    assert_eq!(edit["caret"]["byteOffset"], "0");
    assert!(core
        .execute_command(
            "hex.deleteBackward",
            context,
            serde_json::json!({ "expectedRevision": "0" }),
        )
        .is_err());
}

#[test]
fn plugin_install_approval_and_revision_changes_control_command_visibility() {
    let application_data = tempfile::tempdir().expect("application data");
    let source = tempfile::tempdir().expect("plugin source");
    std::fs::write(
        source.path().join("plugin.json"),
        r#"{
              "id":"nulis.example",
              "name":"Example",
              "version":"1.0.0",
              "engines":{"nulis":">=0.1.0"},
              "permissions":["showMessage"],
              "activationEvents":["onCommand:nulis.example.hello"],
              "main":"index.js",
              "contributes":{"commands":[{"command":"nulis.example.hello","title":"Hello","category":"Examples"}]}
            }"#,
    )
    .expect("manifest");
    std::fs::write(source.path().join("index.js"), "function activate() {}").expect("source");
    let core = NulisCore::with_session_root(application_data.path().into());

    let installed = core
        .execute_command(
            "plugin.install",
            CommandContext::default(),
            serde_json::json!({ "path": source.path() }),
        )
        .expect("install plugin");
    assert_eq!(installed["approved"], false);
    assert!(!core
        .command_descriptors()
        .iter()
        .any(|command| command.id == "nulis.example.hello"));

    core.execute_command(
        "plugin.approve",
        CommandContext::default(),
        serde_json::json!({ "id": "nulis.example", "grant": { "permissions": ["showMessage"] } }),
    )
    .expect("approve plugin");
    let descriptor = core
        .command_descriptors()
        .into_iter()
        .find(|command| command.id == "nulis.example.hello")
        .expect("approved command");
    assert_eq!(descriptor.category, "Plugin · Examples");

    std::fs::write(
        application_data
            .path()
            .join("plugins/nulis.example/index.js"),
        "function activate() { return 1; }",
    )
    .expect("change installed source");
    let listed = core
        .execute_command("plugin.list", CommandContext::default(), Value::Null)
        .expect("list changed plugin");
    assert_eq!(listed[0]["approved"], false);
    assert!(!core
        .command_descriptors()
        .iter()
        .any(|command| command.id == "nulis.example.hello"));
    assert_eq!(
        core.execute_command(
            "plugin.uninstall",
            CommandContext::default(),
            serde_json::json!({ "id": "nulis.example" }),
        )
        .expect("uninstall plugin"),
        Value::Bool(true)
    );
}

#[test]
#[allow(clippy::too_many_lines)] // One end-to-end lifecycle check.
fn declarative_and_executable_plugins_can_be_switched_independently() {
    let application_data = tempfile::tempdir().expect("application data");
    let language_source = tempfile::tempdir().expect("language source");
    std::fs::write(
        language_source.path().join("plugin.json"),
        r#"{
              "apiVersion":2,
              "id":"acme.language",
              "name":"Better JSON",
              "version":"2.0.0",
              "engines":{"nulis":">=0.1.0"},
              "contributes":{"languages":[{
                "id":"better-json",
                "displayName":"Better JSON",
                "extensions":["json"],
                "highlight":{"keywords":["true","false","null"]},
                "completion":{"keywords":true,"words":false}
              }]}
            }"#,
    )
    .expect("language manifest");
    let command_source = tempfile::tempdir().expect("command source");
    std::fs::write(
        command_source.path().join("plugin.json"),
        r#"{
              "id":"nulis.example",
              "name":"Example",
              "version":"1.0.0",
              "engines":{"nulis":">=0.1.0"},
              "permissions":["showMessage"],
              "activationEvents":["onCommand:nulis.example.hello"],
              "main":"index.js",
              "contributes":{"commands":[{"command":"nulis.example.hello","title":"Hello","category":"Examples"}]}
            }"#,
    )
    .expect("command manifest");
    std::fs::write(
        command_source.path().join("index.js"),
        "function activate() {}",
    )
    .expect("command source");

    let core = NulisCore::with_session_root(application_data.path().into());
    let language_id = |core: &NulisCore| {
        core.execute_command("language.list", CommandContext::default(), Value::Null)
            .expect("list languages")
            .as_array()
            .expect("language array")
            .iter()
            .any(|language| language["id"] == "better-json")
    };
    core.execute_command(
        "plugin.install",
        CommandContext::default(),
        serde_json::json!({ "path": language_source.path() }),
    )
    .expect("install language");
    assert!(!language_id(&core));
    core.execute_command(
        "plugin.approve",
        CommandContext::default(),
        serde_json::json!({ "id": "acme.language", "grant": { "permissions": [] } }),
    )
    .expect("approve language");
    assert!(language_id(&core));
    assert_eq!(
        core.execute_command(
            "plugin.languageErrors",
            CommandContext::default(),
            Value::Null
        )
        .expect("language errors"),
        serde_json::json!([])
    );
    let conflicts = core
        .execute_command("language.conflicts", CommandContext::default(), Value::Null)
        .expect("conflicts");
    let json = conflicts
        .as_array()
        .expect("conflict array")
        .iter()
        .find(|conflict| conflict["pattern"] == "*.json")
        .expect("json conflict");
    assert_eq!(json["claims"][0]["source"], "builtin");
    assert_eq!(json["claims"][1]["language"], "better-json");
    assert_eq!(json["claims"][1]["source"], "acme.language");

    core.execute_command(
        "plugin.setCapabilityEnabled",
        CommandContext::default(),
        serde_json::json!({
          "id": "acme.language", "capability": "languages", "enabled": false
        }),
    )
    .expect("switch languages off");
    assert!(!language_id(&core));
    core.execute_command(
        "plugin.setEnabled",
        CommandContext::default(),
        serde_json::json!({ "id": "acme.language", "enabled": true }),
    )
    .expect("enable plugin");
    core.execute_command(
        "plugin.setCapabilityEnabled",
        CommandContext::default(),
        serde_json::json!({
          "id": "acme.language", "capability": "languages", "enabled": true
        }),
    )
    .expect("switch languages on");
    assert!(language_id(&core));

    core.execute_command(
        "plugin.install",
        CommandContext::default(),
        serde_json::json!({ "path": command_source.path() }),
    )
    .expect("install command plugin");
    core.execute_command(
        "plugin.approve",
        CommandContext::default(),
        serde_json::json!({
          "id": "nulis.example", "grant": { "permissions": ["showMessage"] }
        }),
    )
    .expect("approve command plugin");
    let command_visible = |core: &NulisCore| {
        core.command_descriptors()
            .iter()
            .any(|command| command.id == "nulis.example.hello")
    };
    assert!(command_visible(&core));
    core.execute_command(
        "plugin.setCapabilityEnabled",
        CommandContext::default(),
        serde_json::json!({
          "id": "nulis.example", "capability": "commands", "enabled": false
        }),
    )
    .expect("switch commands off");
    assert!(!command_visible(&core));
    // Switching one plugin's commands off leaves the other plugin alone.
    assert!(language_id(&core));

    core.execute_command(
        "plugin.uninstall",
        CommandContext::default(),
        serde_json::json!({ "id": "acme.language" }),
    )
    .expect("uninstall language");
    assert!(!language_id(&core));
    assert_eq!(
        core.execute_command("language.conflicts", CommandContext::default(), Value::Null)
            .expect("conflicts after removal"),
        serde_json::json!([])
    );
}

#[cfg(unix)]
#[test]
fn customization_and_theme_lifecycle_persist_through_core_commands() {
    let directory = tempfile::tempdir().expect("tempdir");
    let core = NulisCore::with_session_root(directory.path().into());
    let themes = core
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list themes");
    assert_eq!(themes.as_array().expect("theme array").len(), 99);

    let mut customization = core
        .execute_command("customization.get", CommandContext::default(), Value::Null)
        .expect("get customization");
    let mut unsafe_customization = customization.clone();
    unsafe_customization["keybindings"][0]["command"] = serde_json::json!("terminal.close");
    assert!(core
        .execute_command(
            "customization.save",
            CommandContext::default(),
            unsafe_customization,
        )
        .is_err());
    customization["appearance"]["fontSize"] = serde_json::json!(17.0);
    core.execute_command(
        "customization.save",
        CommandContext::default(),
        customization,
    )
    .expect("save customization");
    drop(core);

    let reopened = NulisCore::with_session_root(directory.path().into());
    let loaded = reopened
        .execute_command("customization.get", CommandContext::default(), Value::Null)
        .expect("reload customization");
    assert_eq!(loaded["appearance"]["fontSize"], 17.0);

    // Two editors of the same revision: the second save is rejected, not merged.
    let state = reopened
        .execute_command(
            "customization.state",
            CommandContext::default(),
            Value::Null,
        )
        .expect("state");
    let mut first = state["document"].clone();
    first["appearance"]["fontSize"] = serde_json::json!(15.0);
    let saved = reopened
        .execute_command(
            "customization.saveChecked",
            CommandContext::default(),
            serde_json::json!({ "expectedRevision": state["revision"], "document": first }),
        )
        .expect("checked save");
    assert_eq!(
        saved["revision"],
        state["revision"].as_u64().expect("revision") + 1
    );
    let mut second = state["document"].clone();
    second["appearance"]["fontSize"] = serde_json::json!(21.0);
    assert!(reopened
        .execute_command(
            "customization.saveChecked",
            CommandContext::default(),
            serde_json::json!({ "expectedRevision": state["revision"], "document": second }),
        )
        .is_err());
    let current = reopened
        .execute_command("customization.get", CommandContext::default(), Value::Null)
        .expect("current");
    assert_eq!(current["appearance"]["fontSize"], 15.0);
}

#[test]
fn readable_theme_colors_report_the_floors_and_store_nothing() {
    let directory = tempfile::tempdir().expect("tempdir");
    let core = NulisCore::with_session_root(directory.path().into());
    let themes = core
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list themes");
    let shipped = themes.as_array().expect("theme array").len();
    let theme_of = |record: &Value| {
        let mut theme = record.clone();
        if let Some(object) = theme.as_object_mut() {
            object.remove("builtIn");
        }
        theme
    };
    let mut draft = theme_of(&themes.as_array().expect("theme array")[0]);
    draft["id"] = serde_json::json!("example.floored");
    draft["name"] = serde_json::json!("Floored Draft");
    // A bright highlight under the theme's own light text: the floors refuse it, and the
    // panel needs the answer to say so while the color is being picked.
    draft["colors"]["editor.selection"] = serde_json::json!("#FF9300");
    let reply = core
        .execute_command(
            "theme.readable",
            CommandContext::default(),
            serde_json::json!({ "theme": draft }),
        )
        .expect("readable colors for a draft");
    let shown = reply["colors"]["editor.selection"]
        .as_str()
        .expect("shown selection")
        .to_owned();
    assert_ne!(shown, "#FF9300", "{reply}");
    let changes = reply["changes"].as_array().expect("changes");
    assert!(
        changes.iter().any(|change| {
            change["token"] == "editor.selection"
                && change["from"] == "#FF9300"
                && change["to"] == shown
        }),
        "{changes:?}"
    );
    // The floors touch four tokens, so a color outside them is reported exactly as authored.
    assert_eq!(
        reply["colors"]["editor.gutter"],
        draft["colors"]["editor.gutter"]
    );
    assert!(
        !changes
            .iter()
            .any(|change| change["token"] == "editor.gutter"),
        "{changes:?}"
    );
    // Asking stores nothing and changes no answer: the catalog and the draft stand as they were.
    let listed = core
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list after asking");
    assert_eq!(listed.as_array().expect("theme array").len(), shipped);
    assert_eq!(draft["colors"]["editor.selection"], "#FF9300");
}

#[cfg(unix)]
#[test]
#[allow(clippy::too_many_lines)] // One end-to-end custom-theme lifecycle check.
fn custom_theme_edits_persist_through_core_commands_and_protect_built_ins() {
    let directory = tempfile::tempdir().expect("tempdir");
    let core = NulisCore::with_session_root(directory.path().into());
    let themes = core
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list themes");
    let shipped = themes.as_array().expect("theme array").len();
    // theme.list flattens every record: the theme fields sit beside builtIn.
    let theme_of = |record: &Value| {
        let mut theme = record.clone();
        if let Some(object) = theme.as_object_mut() {
            object.remove("builtIn");
        }
        theme
    };
    let built_in = theme_of(&themes.as_array().expect("theme array")[0]);
    let built_in_id = built_in["id"].as_str().expect("built-in id").to_owned();
    let mut custom = built_in.clone();
    custom["id"] = serde_json::json!("example.custom");
    custom["name"] = serde_json::json!("My Custom Theme");
    custom["colors"]["ui.accent"] = serde_json::json!("#F0A500");
    let saved = core
        .execute_command(
            "theme.save",
            CommandContext::default(),
            serde_json::json!({ "theme": custom }),
        )
        .expect("save a custom theme");
    assert_eq!(saved["id"], "example.custom");
    assert_eq!(saved["builtIn"], false);
    let listed = core
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list themes");
    let listed = listed.as_array().expect("theme array");
    assert_eq!(listed.len(), shipped + 1);
    assert!(listed
        .iter()
        .any(|record| record["id"] == "example.custom" && record == &saved));

    // A built-in theme keeps its shipped colors: saving it is refused outright.
    let refused = core
        .execute_command(
            "theme.save",
            CommandContext::default(),
            serde_json::json!({ "theme": built_in }),
        )
        .expect_err("built-ins are read-only");
    assert!(
        format!("{refused}").contains("built-in theme cannot be changed"),
        "{refused}"
    );

    // Selecting the custom theme is a normal customization save, and editing it
    // in place keeps one file for one id.
    let mut customization = core
        .execute_command("customization.get", CommandContext::default(), Value::Null)
        .expect("get customization");
    customization["appearance"]["theme"] = serde_json::json!("example.custom");
    core.execute_command(
        "customization.save",
        CommandContext::default(),
        customization,
    )
    .expect("select the custom theme");
    custom["colors"]["ui.accent"] = serde_json::json!("#00A5F0");
    let edited = core
        .execute_command(
            "theme.save",
            CommandContext::default(),
            serde_json::json!({ "theme": custom }),
        )
        .expect("edit the custom theme");
    let listed = core
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list themes");
    let listed = listed.as_array().expect("theme array");
    assert_eq!(listed.len(), shipped + 1);
    assert!(listed.iter().any(|record| record == &edited));

    // Neither the selected theme nor a built-in can be uninstalled.
    let refused = core
        .execute_command(
            "theme.uninstall",
            CommandContext::default(),
            serde_json::json!({ "id": "example.custom" }),
        )
        .expect_err("the selected theme is protected");
    assert!(
        format!("{refused}").contains("select another theme"),
        "{refused}"
    );
    let refused = core
        .execute_command(
            "theme.uninstall",
            CommandContext::default(),
            serde_json::json!({ "id": built_in_id }),
        )
        .expect_err("built-ins are not user themes");
    assert!(
        format!("{refused}").contains("built-in theme cannot be changed"),
        "{refused}"
    );

    // A restart reads user themes back from the session directory.
    drop(core);
    let reopened = NulisCore::with_session_root(directory.path().into());
    let listed = reopened
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list themes after a restart");
    let listed = listed.as_array().expect("theme array");
    assert_eq!(listed.len(), shipped + 1);
    assert!(listed.iter().any(|record| record["id"] == "example.custom"));

    // Deleting a custom theme needs another theme selected first.
    let mut customization = reopened
        .execute_command("customization.get", CommandContext::default(), Value::Null)
        .expect("get customization");
    customization["appearance"]["theme"] = serde_json::json!(built_in_id);
    reopened
        .execute_command(
            "customization.save",
            CommandContext::default(),
            customization,
        )
        .expect("select a built-in theme");
    reopened
        .execute_command(
            "theme.uninstall",
            CommandContext::default(),
            serde_json::json!({ "id": "example.custom" }),
        )
        .expect("uninstall the custom theme");
    let listed = reopened
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list themes");
    assert_eq!(listed.as_array().expect("theme array").len(), shipped);
}

#[test]
fn applying_a_custom_theme_accepts_the_appearance_the_editor_posts() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let core = NulisCore::with_session_root(directory.path().into());
    let themes = core
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list themes");
    let mut custom = themes.as_array().expect("theme array")[0].clone();
    let theme = custom.as_object_mut().expect("theme object");
    theme.remove("builtIn");
    theme.insert("id".into(), serde_json::json!("example.applied"));
    theme.insert("name".into(), serde_json::json!("Applied Theme"));
    core.execute_command(
        "theme.save",
        CommandContext::default(),
        serde_json::json!({ "theme": custom }),
    )
    .expect("author the custom theme");

    // The Settings window applies a theme by posting the appearance it edited: picking the
    // theme and saving that draft is the "apply" action, and nothing else gates it.
    let state = core
        .execute_command(
            "customization.state",
            CommandContext::default(),
            Value::Null,
        )
        .expect("state customization");
    let mut document = state["document"].clone();
    document["appearance"]["theme"] = serde_json::json!("example.applied");
    core.execute_command(
        "customization.saveChecked",
        CommandContext::default(),
        serde_json::json!({
            "expectedRevision": state["revision"],
            "document": document
        }),
    )
    .expect("select the custom theme");

    drop(core);
    let reopened = NulisCore::with_session_root(directory.path().into());
    let loaded = reopened
        .execute_command("customization.get", CommandContext::default(), Value::Null)
        .expect("reload the customization");
    assert_eq!(loaded["appearance"]["theme"], "example.applied");

    // A customization written while the panel carried the "Apply theme colors" switch keeps
    // its stored flag across the same save and restart: the old document is not rewritten.
    let state = reopened
        .execute_command(
            "customization.state",
            CommandContext::default(),
            Value::Null,
        )
        .expect("state the customization again");
    let mut document = state["document"].clone();
    document["appearance"]["applyUiTheme"] = serde_json::json!(true);
    reopened
        .execute_command(
            "customization.saveChecked",
            CommandContext::default(),
            serde_json::json!({
                "expectedRevision": state["revision"],
                "document": document
            }),
        )
        .expect("save the appearance an older build wrote");
    let loaded = reopened
        .execute_command("customization.get", CommandContext::default(), Value::Null)
        .expect("reload after the older appearance");
    assert_eq!(loaded["appearance"]["applyUiTheme"], true);
    assert_eq!(loaded["appearance"]["theme"], "example.applied");
}

#[test]
fn renaming_a_selected_theme_keeps_the_customization_pointing_at_it() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let core = NulisCore::with_session_root(directory.path().into());
    let themes = core
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list themes");
    let mut custom = themes.as_array().expect("theme array")[0].clone();
    let theme = custom.as_object_mut().expect("theme object");
    theme.remove("builtIn");
    theme.insert("id".into(), serde_json::json!("example.renamed"));
    theme.insert("name".into(), serde_json::json!("Before"));
    core.execute_command(
        "theme.save",
        CommandContext::default(),
        serde_json::json!({ "theme": custom }),
    )
    .expect("author the custom theme");
    let mut customization = core
        .execute_command("customization.get", CommandContext::default(), Value::Null)
        .expect("get customization");
    customization["appearance"]["theme"] = serde_json::json!("example.renamed");
    core.execute_command(
        "customization.save",
        CommandContext::default(),
        customization.clone(),
    )
    .expect("select the custom theme");

    custom["name"] = serde_json::json!("After");
    let renamed = core
        .execute_command(
            "theme.save",
            CommandContext::default(),
            serde_json::json!({ "theme": custom }),
        )
        .expect("rename the custom theme");
    assert_eq!(renamed["id"], "example.renamed");
    assert_eq!(renamed["name"], "After");

    // The customization names the theme by id, so the selection survives the rename.
    core.execute_command(
        "customization.save",
        CommandContext::default(),
        customization,
    )
    .expect("the selection keeps validating against the renamed theme");

    drop(core);
    let reopened = NulisCore::with_session_root(directory.path().into());
    let loaded = reopened
        .execute_command("customization.get", CommandContext::default(), Value::Null)
        .expect("reload the customization");
    assert_eq!(loaded["appearance"]["theme"], "example.renamed");
    let listed = reopened
        .execute_command("theme.list", CommandContext::default(), Value::Null)
        .expect("list themes");
    assert!(listed
        .as_array()
        .expect("theme array")
        .iter()
        .any(|record| { record["id"] == "example.renamed" && record["name"] == "After" }));
}

#[cfg(unix)]
#[test]
fn terminal_lifecycle_is_exposed_through_capability_commands() {
    let directory = tempfile::tempdir().expect("tempdir");
    let core = NulisCore::with_session_root(directory.path().into());
    let created = core
        .execute_command(
            "terminal.create",
            CommandContext::default(),
            serde_json::json!({
                "program": "/bin/sh",
                "arguments": ["-c", "printf command-terminal"],
                "environment": {},
                "size": { "columns": 80, "rows": 24 }
            }),
        )
        .expect("create terminal");
    let terminal_id = created["terminalId"].as_str().expect("terminal ID");
    let output = core
        .execute_command(
            "terminal.read",
            CommandContext::default(),
            serde_json::json!({
                "terminalId": terminal_id,
                "cursor": "0",
                "maximum": 1024
            }),
        )
        .expect("read terminal");
    assert_eq!(output["terminalId"], terminal_id);
    core.execute_command(
        "terminal.close",
        CommandContext::default(),
        serde_json::json!({ "terminalId": terminal_id }),
    )
    .expect("close terminal");
}
