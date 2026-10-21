//! Export to PDF commands (`IMPROVEMENT_PLAN_3.md`).
//!
//! `file.exportPdfTo` exports in one call (for scripts and tests). The
//! desktop uses `file.exportPdfStart`, which runs the export on its own
//! thread, with `file.exportPdfProgress` and `file.exportPdfCancel`, like
//! Count All. One export runs at a time.

use crate::{
    context_document,
    editor::{
        self, DocumentId, DocumentManager, EditorError, ExportObserver, ExportProgress,
        PdfExportRequest, PdfExportResult, Unobserved, LARGE_EXPORT_PAGES,
    },
    editor_command_error, parse_arguments, serialization_command_error, CommandDescriptor,
    CommandError, CommandRegistry,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

/// Longest `{date}` text accepted, in bytes.
const MAX_EXPORT_DATE_BYTES: usize = 128;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportPdfArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    path: String,
    #[serde(default)]
    setup: nulis_print::PageSetup,
    #[serde(default)]
    colors: nulis_print::ColorChoice,
    /// The date for `{date}`, formatted by the desktop in the user's locale.
    #[serde(default)]
    date: String,
    /// The user already agreed to a very large export.
    #[serde(default)]
    confirmed: bool,
    /// The whole document (default) or the selection.
    #[serde(default)]
    scope: editor::ExportScope,
}

impl ExportPdfArguments {
    /// The request, asking first above [`LARGE_EXPORT_PAGES`] unless `ask` is false.
    fn request(self, ask: bool) -> Result<PdfExportRequest, CommandError> {
        if self.date.len() > MAX_EXPORT_DATE_BYTES {
            return Err(CommandError::Failed("the date is too long".into()));
        }
        Ok(PdfExportRequest {
            path: std::path::PathBuf::from(self.path),
            expected_revision: self.expected_revision,
            setup: self.setup,
            colors: self.colors,
            date: self.date,
            confirm_above: (ask && !self.confirmed).then_some(LARGE_EXPORT_PAGES),
            scope: self.scope,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportJobArguments {
    job_id: u64,
}

/// What an export job has done so far.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportReport {
    progress: ExportProgress,
    done: bool,
    cancelled: bool,
    /// The fewest pages, as a decimal string, when the export stopped to
    /// ask about a very large PDF.
    #[serde(skip_serializing_if = "Option::is_none")]
    needs_confirmation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<PdfExportResult>,
}

#[derive(Debug, Default)]
struct ExportJob {
    cancel: AtomicBool,
    report: Mutex<ExportReport>,
}

impl ExportObserver for ExportJob {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn report(&self, progress: ExportProgress) {
        if let Ok(mut report) = self.report.lock() {
            report.progress = progress;
        }
    }
}

#[derive(Debug, Default)]
struct ExportJobs {
    next: AtomicU64,
    jobs: Mutex<HashMap<u64, Arc<ExportJob>>>,
}

fn start(
    documents: &Arc<DocumentManager>,
    jobs: &ExportJobs,
    document_id: DocumentId,
    request: PdfExportRequest,
) -> Result<u64, CommandError> {
    let job = Arc::new(ExportJob::default());
    let id = jobs.next.fetch_add(1, Ordering::Relaxed) + 1;
    {
        let mut running = jobs
            .jobs
            .lock()
            .map_err(|_| CommandError::Unavailable("export jobs".into()))?;
        // One export at a time: a new one stops the one before.
        for previous in running.values() {
            previous.cancel.store(true, Ordering::Relaxed);
        }
        running.clear();
        running.insert(id, Arc::clone(&job));
    }
    let documents = Arc::clone(documents);
    std::thread::Builder::new()
        .name(format!("nulis-export-{id}"))
        .spawn(move || {
            let outcome = documents.export_pdf(&document_id, &request, job.as_ref());
            if let Ok(mut report) = job.report.lock() {
                match outcome {
                    Ok(result) => report.result = Some(result),
                    Err(EditorError::ExportCancelled) => report.cancelled = true,
                    Err(EditorError::ExportNeedsConfirmation { estimated_pages }) => {
                        report.needs_confirmation = Some(estimated_pages.to_string());
                    }
                    Err(error) => report.error = Some(error.to_string()),
                }
                report.done = true;
            }
        })
        .map_err(|error| CommandError::Failed(format!("could not start the export: {error}")))?;
    Ok(id)
}

pub(crate) fn register_print_commands(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new("file.exportPdfTo", "Export to PDF File", "File")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: ExportPdfArguments = parse_arguments(arguments)?;
                let result = manager
                    .export_pdf(&document_id, &arguments.request(false)?, &Unobserved)
                    .map_err(|error| editor_command_error(&error))?;
                serde_json::to_value(result).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let jobs = Arc::new(ExportJobs::default());
    let (manager, export_jobs) = (Arc::clone(documents), Arc::clone(&jobs));
    commands
        .register(
            CommandDescriptor::new("file.exportPdfStart", "Start Export to PDF", "File")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: ExportPdfArguments = parse_arguments(arguments)?;
                let id = start(
                    &manager,
                    &export_jobs,
                    document_id,
                    arguments.request(true)?,
                )?;
                Ok(json!({ "jobId": id }))
            },
        )
        .expect("built-in command is unique");

    let export_jobs = Arc::clone(&jobs);
    commands
        .register(
            CommandDescriptor::new("file.exportPdfProgress", "Export to PDF Progress", "File")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: ExportJobArguments = parse_arguments(arguments)?;
                let mut running = export_jobs
                    .jobs
                    .lock()
                    .map_err(|_| CommandError::Unavailable("export jobs".into()))?;
                let job = running.get(&arguments.job_id).cloned().ok_or_else(|| {
                    CommandError::Failed("this export is no longer running".into())
                })?;
                let report = job
                    .report
                    .lock()
                    .map_err(|_| CommandError::Unavailable("export report".into()))?
                    .clone();
                if report.done {
                    running.remove(&arguments.job_id);
                }
                serde_json::to_value(report).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let export_jobs = jobs;
    commands
        .register(
            CommandDescriptor::new("file.exportPdfCancel", "Cancel Export to PDF", "File")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: ExportJobArguments = parse_arguments(arguments)?;
                if let Ok(running) = export_jobs.jobs.lock() {
                    if let Some(job) = running.get(&arguments.job_id) {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                }
                Ok(serde_json::Value::Null)
            },
        )
        .expect("built-in command is unique");
}
