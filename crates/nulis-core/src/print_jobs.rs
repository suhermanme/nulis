//! Export to PDF and Print commands (`IMPROVEMENT_PLAN_3.md`).
//!
//! `file.exportPdfTo` exports in one call (for scripts and tests). The
//! desktop uses `file.exportPdfStart`, which runs the export on its own
//! thread, with `file.exportPdfProgress` and `file.exportPdfCancel`, like
//! Count All. `file.printStart` lays out pages for Print the same way and
//! shares the progress and cancel commands; one job of either kind runs at
//! a time. The finished pages wait in the core until the Print Preview
//! window reads them with `print.pages`, and are dropped by `print.release`
//! or the next Print.

use crate::{
    context_document,
    editor::{
        self, DocumentId, DocumentManager, EditorError, ExportObserver, ExportProgress,
        PdfExportRequest, PdfExportResult, PrintRequest, PrintedDocument, Unobserved,
        LARGE_EXPORT_PAGES, MAX_PRINT_PAGES,
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
struct PrintArguments {
    #[serde(with = "editor::decimal_u64")]
    expected_revision: u64,
    #[serde(default)]
    setup: nulis_print::PageSetup,
    #[serde(default)]
    colors: nulis_print::ColorChoice,
    #[serde(default)]
    date: String,
    #[serde(default)]
    confirmed: bool,
    #[serde(default)]
    scope: editor::ExportScope,
}

impl PrintArguments {
    fn request(self) -> Result<PrintRequest, CommandError> {
        if self.date.len() > MAX_EXPORT_DATE_BYTES {
            return Err(CommandError::Failed("the date is too long".into()));
        }
        Ok(PrintRequest {
            expected_revision: self.expected_revision,
            setup: self.setup,
            colors: self.colors,
            date: self.date,
            confirm_above: (!self.confirmed).then_some(LARGE_EXPORT_PAGES),
            max_pages: Some(MAX_PRINT_PAGES),
            scope: self.scope,
        })
    }
}

/// Most pages `print.pages` returns at once.
const MAX_PAGES_PER_READ: usize = 100;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PrintPagesArguments {
    start: usize,
    count: usize,
}

/// Pages laid out for Print, waiting for the Print Preview window.
#[derive(Debug, Default)]
struct PreparedPrint {
    /// Which Print they come from, so the window notices a newer one.
    id: u64,
    document: Option<Arc<PrintedDocument>>,
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
    /// Pages laid out by a Print, as a decimal string; read them with `print.pages`.
    #[serde(skip_serializing_if = "Option::is_none")]
    printed_pages: Option<String>,
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
    prepared: Mutex<PreparedPrint>,
}

/// How a job ended, for its report.
enum JobOutcome {
    Exported(PdfExportResult),
    Printed(u64),
}

/// Runs `work` on its own thread as the only job, and records how it ended.
fn start(
    jobs: &Arc<ExportJobs>,
    work: impl FnOnce(&ExportJob, &ExportJobs, u64) -> Result<JobOutcome, EditorError> + Send + 'static,
) -> Result<u64, CommandError> {
    let job = Arc::new(ExportJob::default());
    let id = jobs.next.fetch_add(1, Ordering::Relaxed) + 1;
    {
        let mut running = jobs
            .jobs
            .lock()
            .map_err(|_| CommandError::Unavailable("export jobs".into()))?;
        // One job at a time: a new one stops the one before.
        for previous in running.values() {
            previous.cancel.store(true, Ordering::Relaxed);
        }
        running.clear();
        running.insert(id, Arc::clone(&job));
    }
    let jobs = Arc::clone(jobs);
    std::thread::Builder::new()
        .name(format!("nulis-export-{id}"))
        .spawn(move || {
            let outcome = work(job.as_ref(), &jobs, id);
            if let Ok(mut report) = job.report.lock() {
                match outcome {
                    Ok(JobOutcome::Exported(result)) => report.result = Some(result),
                    Ok(JobOutcome::Printed(pages)) => {
                        report.printed_pages = Some(pages.to_string());
                    }
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

/// Lays out pages for Print and keeps them for the Print Preview window.
fn print_job(
    documents: &DocumentManager,
    document_id: &DocumentId,
    request: &PrintRequest,
    job: &ExportJob,
    jobs: &ExportJobs,
    id: u64,
) -> Result<JobOutcome, EditorError> {
    let printed = documents.print_pages(document_id, request, job)?;
    let pages = u64::try_from(printed.pages.len()).unwrap_or(u64::MAX);
    // A Print started later has already replaced this one's pages.
    if job.cancelled() {
        return Err(EditorError::ExportCancelled);
    }
    let mut prepared = jobs
        .prepared
        .lock()
        .map_err(|_| EditorError::StateUnavailable)?;
    *prepared = PreparedPrint {
        id,
        document: Some(Arc::new(printed)),
    };
    Ok(JobOutcome::Printed(pages))
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
                let request = arguments.request(true)?;
                let manager = Arc::clone(&manager);
                let id = start(&export_jobs, move |job, _, _| {
                    manager
                        .export_pdf(&document_id, &request, job)
                        .map(JobOutcome::Exported)
                })?;
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

    register_print_page_commands(commands, documents, &jobs);

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

/// Print: lay out pages as a job, preview the first page, and hand the
/// pages to the Print Preview window.
fn register_print_page_commands(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
    jobs: &Arc<ExportJobs>,
) {
    let (manager, export_jobs) = (Arc::clone(documents), Arc::clone(jobs));
    commands
        .register(
            CommandDescriptor::new("file.printStart", "Prepare Pages to Print", "File")
                .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: PrintArguments = parse_arguments(arguments)?;
                let request = arguments.request()?;
                let manager = Arc::clone(&manager);
                let id = start(&export_jobs, move |job, jobs, id| {
                    print_job(&manager, &document_id, &request, job, jobs, id)
                })?;
                Ok(json!({ "jobId": id }))
            },
        )
        .expect("built-in command is unique");

    let manager = Arc::clone(documents);
    commands
        .register(
            CommandDescriptor::new(
                "file.printPreview",
                "Preview the First Printed Page",
                "File",
            )
            .expect("built-in command ID is valid"),
            move |context, arguments| {
                let document_id = context_document(&context)?;
                let arguments: PrintArguments = parse_arguments(arguments)?;
                let (frame, page) = manager
                    .print_preview(&document_id, &arguments.request()?)
                    .map_err(|error| editor_command_error(&error))?;
                Ok(json!({ "frame": frame, "page": page }))
            },
        )
        .expect("built-in command is unique");

    let export_jobs = Arc::clone(jobs);
    commands
        .register(
            CommandDescriptor::new("print.pages", "Read Pages to Print", "File")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: PrintPagesArguments = parse_arguments(arguments)?;
                let (id, document) = {
                    let prepared = export_jobs
                        .prepared
                        .lock()
                        .map_err(|_| CommandError::Unavailable("pages to print".into()))?;
                    (prepared.id, prepared.document.clone())
                };
                let document = document
                    .ok_or_else(|| CommandError::Failed("there is nothing to print".into()))?;
                let total = document.pages.len();
                let start = arguments.start.min(total);
                let end = start + arguments.count.min(MAX_PAGES_PER_READ).min(total - start);
                Ok(json!({
                    "printId": id,
                    "title": document.title,
                    "frame": document.frame,
                    "total": total,
                    "start": start,
                    "pages": &document.pages[start..end],
                }))
            },
        )
        .expect("built-in command is unique");

    let export_jobs = Arc::clone(jobs);
    commands
        .register(
            CommandDescriptor::new("print.release", "Release Pages to Print", "File")
                .expect("built-in command ID is valid"),
            move |_context, _arguments| {
                if let Ok(mut prepared) = export_jobs.prepared.lock() {
                    prepared.document = None;
                }
                Ok(serde_json::Value::Null)
            },
        )
        .expect("built-in command is unique");
}
