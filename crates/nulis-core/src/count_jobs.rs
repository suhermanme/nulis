//! Count All as a background job: counts every match, with no match limit,
//! across one or more documents, slice by slice so the editor stays free,
//! with progress and cancellation.
//!
//! `search.countStart` begins a count (and stops any count already running),
//! `search.countProgress` reports it, and `search.countCancel` stops it.

use crate::{
    editor::{DocumentId, DocumentManager},
    editor_command_error, parse_arguments, serialization_command_error, ByteRangeArguments,
    CommandDescriptor, CommandError, CommandRegistry,
};
use nulis_search::{CancellationToken, CountProgress, SearchOptions};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

/// Most documents one count may cover (all open files).
const MAX_COUNT_TARGETS: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CountTarget {
    document_id: String,
    /// A byte range of the document, for "selected text".
    #[serde(default)]
    within: Option<ByteRangeArguments>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CountStartArguments {
    query: String,
    #[serde(default)]
    options: SearchOptions,
    targets: Vec<CountTarget>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CountJobArguments {
    job_id: u64,
}

/// What a count has found so far.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct CountReport {
    #[serde(with = "crate::editor::decimal_u64")]
    matches: u64,
    #[serde(with = "crate::editor::decimal_u64")]
    lines: u64,
    #[serde(with = "crate::editor::decimal_u64")]
    scanned_bytes: u64,
    #[serde(with = "crate::editor::decimal_u64")]
    total_bytes: u64,
    documents: usize,
    documents_with_matches: usize,
    done: bool,
    cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug)]
struct CountJob {
    cancel: CancellationToken,
    report: Mutex<CountReport>,
}

#[derive(Debug, Default)]
struct CountJobs {
    next: AtomicU64,
    jobs: Mutex<HashMap<u64, Arc<CountJob>>>,
}

/// One document's part of a count: where it starts and ends, and the
/// revision it was counted at.
struct Plan {
    document_id: DocumentId,
    start: u64,
    end: u64,
    revision: u64,
}

fn run(
    documents: &DocumentManager,
    job: &CountJob,
    query: &str,
    options: SearchOptions,
    plans: Vec<Plan>,
) {
    let mut scanned_before = 0_u64;
    let (mut matches, mut lines, mut with_matches) = (0_u64, 0_u64, 0_usize);
    let publish = |update: &dyn Fn(&mut CountReport)| {
        if let Ok(mut report) = job.report.lock() {
            update(&mut report);
        }
    };
    for plan in plans {
        let mut progress = CountProgress::starting_at(plan.start);
        loop {
            if job.cancel.is_cancelled() {
                publish(&|report| {
                    report.cancelled = true;
                    report.done = true;
                });
                return;
            }
            if let Err(error) = documents.count_step(
                &plan.document_id,
                plan.revision,
                query,
                options,
                plan.end,
                &mut progress,
                &job.cancel,
            ) {
                let message = error.to_string();
                publish(&|report| {
                    report.error = Some(message.clone());
                    report.done = true;
                });
                return;
            }
            let scanned = scanned_before + progress.next.saturating_sub(plan.start);
            let (found, found_lines) = (matches + progress.matches, lines + progress.lines);
            publish(&|report| {
                report.matches = found;
                report.lines = found_lines;
                report.scanned_bytes = scanned;
            });
            if progress.done {
                break;
            }
        }
        matches += progress.matches;
        lines += progress.lines;
        with_matches += usize::from(progress.matches > 0);
        scanned_before += plan.end - plan.start;
        publish(&|report| report.documents_with_matches = with_matches);
    }
    publish(&|report| report.done = true);
}

fn start(
    documents: &Arc<DocumentManager>,
    jobs: &CountJobs,
    arguments: CountStartArguments,
) -> Result<u64, CommandError> {
    if arguments.query.is_empty() {
        return Err(CommandError::Failed("enter text to count".into()));
    }
    if arguments.targets.is_empty() || arguments.targets.len() > MAX_COUNT_TARGETS {
        return Err(CommandError::Failed(format!(
            "a count covers 1 to {MAX_COUNT_TARGETS} documents"
        )));
    }
    let query_bytes = arguments.query.as_bytes();
    nulis_search::validate_pattern(query_bytes, arguments.options)
        .map_err(|error| CommandError::Failed(error.to_string()))?;
    let mut plans = Vec::with_capacity(arguments.targets.len());
    for target in &arguments.targets {
        let document_id = DocumentId(target.document_id.clone());
        let (len, revision) = documents
            .length_and_revision(&document_id)
            .map_err(|error| editor_command_error(&error))?;
        let (start, end) = target.within.map_or((0, len), ByteRangeArguments::pair);
        let end = end.min(len);
        plans.push(Plan {
            document_id,
            start: start.min(end),
            end,
            revision,
        });
    }
    let job = Arc::new(CountJob {
        cancel: CancellationToken::default(),
        report: Mutex::new(CountReport {
            total_bytes: plans.iter().map(|plan| plan.end - plan.start).sum(),
            documents: plans.len(),
            ..CountReport::default()
        }),
    });
    let id = jobs.next.fetch_add(1, Ordering::Relaxed) + 1;
    {
        let mut running = jobs
            .jobs
            .lock()
            .map_err(|_| CommandError::Unavailable("count jobs".into()))?;
        // One count at a time: a new Count All replaces the one before.
        for previous in running.values() {
            previous.cancel.cancel();
        }
        running.clear();
        running.insert(id, Arc::clone(&job));
    }
    let documents = Arc::clone(documents);
    let (query, options) = (arguments.query, arguments.options);
    std::thread::Builder::new()
        .name(format!("nulis-count-{id}"))
        .spawn(move || run(&documents, &job, &query, options, plans))
        .map_err(|error| CommandError::Failed(format!("could not start counting: {error}")))?;
    Ok(id)
}

pub(crate) fn register_count_commands(
    commands: &mut CommandRegistry,
    documents: &Arc<DocumentManager>,
) {
    let jobs = Arc::new(CountJobs::default());

    let (manager, count_jobs) = (Arc::clone(documents), Arc::clone(&jobs));
    commands
        .register(
            CommandDescriptor::new("search.countStart", "Count All", "Search")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: CountStartArguments = parse_arguments(arguments)?;
                let id = start(&manager, &count_jobs, arguments)?;
                Ok(json!({ "jobId": id }))
            },
        )
        .expect("built-in command is unique");

    let count_jobs = Arc::clone(&jobs);
    commands
        .register(
            CommandDescriptor::new("search.countProgress", "Count Progress", "Search")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: CountJobArguments = parse_arguments(arguments)?;
                let mut running = count_jobs
                    .jobs
                    .lock()
                    .map_err(|_| CommandError::Unavailable("count jobs".into()))?;
                let job = running.get(&arguments.job_id).cloned().ok_or_else(|| {
                    CommandError::Failed("this count is no longer running".into())
                })?;
                let report = job
                    .report
                    .lock()
                    .map_err(|_| CommandError::Unavailable("count report".into()))?
                    .clone();
                if report.done {
                    running.remove(&arguments.job_id);
                }
                serde_json::to_value(report).map_err(|error| serialization_command_error(&error))
            },
        )
        .expect("built-in command is unique");

    let count_jobs = jobs;
    commands
        .register(
            CommandDescriptor::new("search.countCancel", "Cancel Count", "Search")
                .expect("built-in command ID is valid"),
            move |_context, arguments| {
                let arguments: CountJobArguments = parse_arguments(arguments)?;
                if let Ok(running) = count_jobs.jobs.lock() {
                    if let Some(job) = running.get(&arguments.job_id) {
                        job.cancel.cancel();
                    }
                }
                Ok(serde_json::Value::Null)
            },
        )
        .expect("built-in command is unique");
}
