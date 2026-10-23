//! Language servers the user configured and approved.
//!
//! Servers are listed only in the user's `settings.json` (`lsp.servers`),
//! each with an argument vector and the languages it serves, and run only
//! after the user approves that exact command and language list; changing
//! either revokes the approval. A server starts when a document of its
//! language first needs it, one per project folder, and stops when its
//! last document closes. A server that fails three times in a row is set
//! aside until the user restarts it. Document text is copied before every
//! call, so no editor state is held while a server works.

use crate::editor::{DocumentId, DocumentManager, ServerDocument};
use nulis_lsp::{file_uri, offset_of, position_of, LspClient, LspError, PositionEncoding};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const START_TIMEOUT: Duration = Duration::from_secs(20);
const COMPLETION_TIMEOUT: Duration = Duration::from_millis(1_500);
const FORMAT_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_FAILURES: u32 = 3;
const MAX_SERVERS: usize = 32;
const MAX_COMMAND_PARTS: usize = 64;
const MAX_COMMAND_PART_BYTES: usize = 4_096;
const MAX_DIAGNOSTICS: usize = 2_000;
const MAX_COMPLETIONS: usize = 200;
/// Files that mark a project's top folder, where a server is started.
const ROOT_MARKERS: &[&str] = &[
    ".git",
    "Cargo.toml",
    "package.json",
    "go.mod",
    "pyproject.toml",
    "compile_commands.json",
    ".clangd",
];

/// One `lsp.servers` entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerConfig {
    /// The program, then its arguments.
    pub command: Vec<String>,
    /// Language IDs this server is used for.
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initialization_options: Option<Value>,
}

/// What the user approved: the server ID with its exact command and languages.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Approval {
    id: String,
    command: Vec<String>,
    languages: Vec<String>,
}

/// A server as Settings lists it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub id: String,
    pub command: Vec<String>,
    pub languages: Vec<String>,
    pub approved: bool,
    /// Project folders it runs for now.
    pub running: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Set aside after repeated failures, until restarted.
    pub stopped: bool,
}

/// A problem a server reported, placed in the document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    #[serde(with = "crate::editor::decimal_u64")]
    pub start_byte: u64,
    #[serde(with = "crate::editor::decimal_u64")]
    pub end_byte: u64,
    /// One-based line of the start.
    pub line: u64,
    /// `error`, `warning`, `information`, or `hint`.
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The answer to `lsp.diagnostics`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentProblems {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// The document revision the problems were found in.
    #[serde(with = "crate::editor::decimal_u64")]
    pub revision: u64,
    pub problems: Vec<Problem>,
}

/// A document a running server has open.
#[derive(Debug)]
struct OpenDocument {
    uri: String,
    version: i64,
    /// The text and revision last sent, used to place problems.
    sent: ServerDocument,
    problems: Vec<Problem>,
}

#[derive(Debug)]
struct Running {
    client: Arc<LspClient>,
    documents: BTreeMap<DocumentId, OpenDocument>,
}

#[derive(Debug, Default)]
struct Failure {
    count: u32,
    retry_after: Option<Instant>,
    error: String,
}

#[derive(Debug, Default)]
struct State {
    configs: BTreeMap<String, ServerConfig>,
    approvals: Vec<Approval>,
    running: BTreeMap<(String, PathBuf), Running>,
    failures: BTreeMap<String, Failure>,
    /// Problems in settings.json's `lsp.servers`, reported by `lsp.state`.
    config_error: Option<String>,
}

/// Every configured language server and the ones running.
#[derive(Debug)]
pub struct LanguageServers {
    approvals_path: PathBuf,
    start_timeout: Mutex<Duration>,
    state: Mutex<State>,
}

fn validate_config(id: &str, config: &ServerConfig) -> Result<(), String> {
    let valid_id = !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.' | b'_')
        });
    if !valid_id {
        return Err(format!(
            "{id:?} is not a valid server name (lowercase letters, digits, - . _)"
        ));
    }
    if config.command.is_empty()
        || config.command.len() > MAX_COMMAND_PARTS
        || config.command.iter().any(|part| {
            part.is_empty() || part.len() > MAX_COMMAND_PART_BYTES || part.contains('\0')
        })
    {
        return Err(format!(
            "{id}: the command must be a list of 1 to {MAX_COMMAND_PARTS} non-empty strings"
        ));
    }
    if config.languages.is_empty() || config.languages.len() > 64 {
        return Err(format!("{id}: list 1 to 64 language IDs"));
    }
    Ok(())
}

/// The project folder for a file: the nearest folder above it holding a
/// marker such as `.git` or `Cargo.toml`, else the file's own folder.
fn project_root(path: &Path) -> PathBuf {
    let folder = path.parent().unwrap_or(path);
    folder
        .ancestors()
        .find(|candidate| {
            ROOT_MARKERS
                .iter()
                .any(|marker| candidate.join(marker).exists())
        })
        .unwrap_or(folder)
        .to_path_buf()
}

fn severity(value: &Value) -> String {
    match value.as_u64() {
        Some(1) | None => "error",
        Some(2) => "warning",
        Some(3) => "information",
        _ => "hint",
    }
    .to_owned()
}

impl LanguageServers {
    #[must_use]
    pub fn new(approvals_path: PathBuf) -> Self {
        let approvals = std::fs::read(&approvals_path)
            .ok()
            .filter(|bytes| bytes.len() <= 256 * 1024)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            approvals_path,
            start_timeout: Mutex::new(START_TIMEOUT),
            state: Mutex::new(State {
                approvals,
                ..State::default()
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Reads `lsp.servers` from effective settings. Invalid entries are left
    /// out and reported; servers whose configuration changed are stopped.
    /// Returns language ID to approved server ID.
    pub fn configure(&self, settings: &Value) -> BTreeMap<String, String> {
        let mut configs = BTreeMap::new();
        let mut problems = Vec::new();
        if let Some(entries) = settings["lsp"]["servers"].as_object() {
            for (id, entry) in entries.iter().take(MAX_SERVERS) {
                match serde_json::from_value::<ServerConfig>(entry.clone())
                    .map_err(|error| format!("{id}: {error}"))
                    .and_then(|config| validate_config(id, &config).map(|()| config))
                {
                    Ok(config) => {
                        configs.insert(id.clone(), config);
                    }
                    Err(problem) => problems.push(problem),
                }
            }
        }
        let mut state = self.state();
        let outdated: Vec<(String, PathBuf)> = state
            .running
            .keys()
            .filter(|(id, _)| state.configs.get(id) != configs.get(id))
            .cloned()
            .collect();
        for key in outdated {
            if let Some(running) = state.running.remove(&key) {
                running.client.shutdown(SHUTDOWN_TIMEOUT);
            }
        }
        state.configs = configs;
        state.config_error = (!problems.is_empty()).then(|| problems.join("; "));
        Self::language_map(&state)
    }

    fn approved(state: &State, id: &str) -> bool {
        state.configs.get(id).is_some_and(|config| {
            state.approvals.iter().any(|approval| {
                approval.id == id
                    && approval.command == config.command
                    && approval.languages == config.languages
            })
        })
    }

    fn language_map(state: &State) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        for (id, config) in &state.configs {
            if Self::approved(state, id) {
                for language in &config.languages {
                    map.entry(language.clone()).or_insert_with(|| id.clone());
                }
            }
        }
        map
    }

    /// Every configured server with its approval and running state.
    #[must_use]
    pub fn status(&self) -> (Vec<ServerStatus>, Option<String>) {
        let state = self.state();
        let servers = state
            .configs
            .iter()
            .map(|(id, config)| {
                let failure = state.failures.get(id);
                ServerStatus {
                    id: id.clone(),
                    command: config.command.clone(),
                    languages: config.languages.clone(),
                    approved: Self::approved(&state, id),
                    running: state
                        .running
                        .keys()
                        .filter(|(server, _)| server == id)
                        .map(|(_, root)| root.to_string_lossy().into_owned())
                        .collect(),
                    error: failure
                        .map(|failure| failure.error.clone())
                        .filter(|error| !error.is_empty()),
                    stopped: failure.is_some_and(|failure| failure.count >= MAX_FAILURES),
                }
            })
            .collect();
        (servers, state.config_error.clone())
    }

    /// Approves or revokes a configured server; revoking stops it. Returns
    /// the new language map.
    ///
    /// # Errors
    /// Returns an error for an unknown server or a failed write.
    pub fn set_approved(
        &self,
        id: &str,
        approved: bool,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut state = self.state();
        let config = state
            .configs
            .get(id)
            .cloned()
            .ok_or_else(|| format!("no language server named {id:?} in settings.json"))?;
        state.approvals.retain(|approval| approval.id != id);
        if approved {
            state.approvals.push(Approval {
                id: id.to_owned(),
                command: config.command,
                languages: config.languages,
            });
            state.failures.remove(id);
        } else {
            let keys: Vec<_> = state
                .running
                .keys()
                .filter(|(server, _)| server == id)
                .cloned()
                .collect();
            for key in keys {
                if let Some(running) = state.running.remove(&key) {
                    running.client.shutdown(SHUTDOWN_TIMEOUT);
                }
            }
        }
        let bytes =
            serde_json::to_vec_pretty(&state.approvals).map_err(|error| error.to_string())?;
        if let Some(parent) = self.approvals_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut file = tempfile::NamedTempFile::new_in(
            self.approvals_path
                .parent()
                .unwrap_or_else(|| Path::new(".")),
        )
        .map_err(|error| error.to_string())?;
        std::io::Write::write_all(&mut file, &bytes).map_err(|error| error.to_string())?;
        file.persist(&self.approvals_path)
            .map_err(|error| error.error.to_string())?;
        Ok(Self::language_map(&state))
    }

    /// Stops a server everywhere and clears its failures, so the next
    /// document that needs it starts it again.
    pub fn restart(&self, id: &str) {
        let mut state = self.state();
        state.failures.remove(id);
        let keys: Vec<_> = state
            .running
            .keys()
            .filter(|(server, _)| server == id)
            .cloned()
            .collect();
        for key in keys {
            if let Some(running) = state.running.remove(&key) {
                running.client.shutdown(SHUTDOWN_TIMEOUT);
            }
        }
    }

    /// Closes documents that are no longer open; a server left without
    /// documents stops.
    pub fn prune(&self, open: &[DocumentId]) {
        let mut state = self.state();
        let mut idle = Vec::new();
        for (key, running) in &mut state.running {
            let closed: Vec<DocumentId> = running
                .documents
                .keys()
                .filter(|id| !open.contains(id))
                .cloned()
                .collect();
            for id in closed {
                if let Some(document) = running.documents.remove(&id) {
                    let _ = running.client.notify(
                        "textDocument/didClose",
                        json!({ "textDocument": { "uri": document.uri } }),
                    );
                }
            }
            if running.documents.is_empty() {
                idle.push(key.clone());
            }
        }
        for key in idle {
            if let Some(running) = state.running.remove(&key) {
                running.client.shutdown(SHUTDOWN_TIMEOUT);
            }
        }
    }

    /// The running client for a document (starting the server if needed)
    /// with the document's text sent to it.
    fn client_for(
        &self,
        id: &DocumentId,
        document: &ServerDocument,
    ) -> Result<Option<(String, Arc<LspClient>)>, String> {
        let mut state = self.state();
        let Some(server) = Self::language_map(&state).get(&document.language).cloned() else {
            return Ok(None);
        };
        let root = project_root(&document.path);
        let key = (server.clone(), root.clone());
        if state
            .running
            .get(&key)
            .is_some_and(|running| !running.client.is_running())
        {
            state.running.remove(&key);
            let failure = state.failures.entry(server.clone()).or_default();
            failure.count += 1;
            "the language server stopped".clone_into(&mut failure.error);
            failure.retry_after =
                Some(Instant::now() + Duration::from_secs(1 << failure.count.min(6)));
        }
        if !state.running.contains_key(&key) {
            let failure = state.failures.get(&server);
            if failure.is_some_and(|failure| {
                failure.count >= MAX_FAILURES
                    || failure
                        .retry_after
                        .is_some_and(|until| Instant::now() < until)
            }) {
                return Ok(None);
            }
            let config = state.configs[&server].clone();
            // Starting can take a while; nothing else waits on documents meanwhile.
            let timeout = *self
                .start_timeout
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match LspClient::start(
                &config.command,
                &root,
                config.initialization_options.as_ref(),
                timeout,
            ) {
                Ok(client) => {
                    state.failures.remove(&server);
                    state.running.insert(
                        key.clone(),
                        Running {
                            client: Arc::new(client),
                            documents: BTreeMap::new(),
                        },
                    );
                }
                Err(error) => {
                    let failure = state.failures.entry(server.clone()).or_default();
                    failure.count += 1;
                    failure.error = error.to_string();
                    failure.retry_after =
                        Some(Instant::now() + Duration::from_secs(1 << failure.count.min(6)));
                    return Err(error.to_string());
                }
            }
        }
        let running = state
            .running
            .get_mut(&key)
            .ok_or("the server is not running")?;
        let client = Arc::clone(&running.client);
        Self::send_text(running, id, document)?;
        Ok(Some((server, client)))
    }

    /// Sends the document's text: opens it, or sends a change when the
    /// revision moved on since the last time.
    fn send_text(
        running: &mut Running,
        id: &DocumentId,
        document: &ServerDocument,
    ) -> Result<(), String> {
        let client = &running.client;
        let uri = file_uri(&document.path);
        match running.documents.get_mut(id) {
            Some(open) if open.sent.revision == document.revision && open.uri == uri => {}
            Some(open) if open.uri == uri => {
                open.version += 1;
                client
                    .notify(
                        "textDocument/didChange",
                        json!({ "textDocument": { "uri": uri, "version": open.version },
                            "contentChanges": [{ "text": document.text }] }),
                    )
                    .map_err(|error| error.to_string())?;
                open.sent = document.clone();
            }
            _ => {
                if let Some(old) = running.documents.remove(id) {
                    let _ = client.notify(
                        "textDocument/didClose",
                        json!({ "textDocument": { "uri": old.uri } }),
                    );
                }
                client
                    .notify(
                        "textDocument/didOpen",
                        json!({ "textDocument": { "uri": uri, "languageId": document.language,
                            "version": 1, "text": document.text } }),
                    )
                    .map_err(|error| error.to_string())?;
                running.documents.insert(
                    id.clone(),
                    OpenDocument {
                        uri,
                        version: 1,
                        sent: document.clone(),
                        problems: Vec::new(),
                    },
                );
            }
        }
        Ok(())
    }

    /// Takes the problems servers have published since the last call.
    fn collect_problems(&self) {
        let mut state = self.state();
        for running in state.running.values_mut() {
            let encoding = running.client.encoding();
            for message in running.client.take_notifications() {
                if message["method"] != "textDocument/publishDiagnostics" {
                    continue;
                }
                let uri = message["params"]["uri"].as_str().unwrap_or_default();
                for open in running
                    .documents
                    .values_mut()
                    .filter(|open| open.uri == uri)
                {
                    open.problems = message["params"]["diagnostics"]
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .take(MAX_DIAGNOSTICS)
                                .map(|item| place_problem(&open.sent, encoding, item))
                                .collect()
                        })
                        .unwrap_or_default();
                }
            }
        }
    }

    /// The problems the document's server reports, after sending it the
    /// current text. Problems arrive a little after the text, so the
    /// editor asks again shortly after each change.
    ///
    /// # Errors
    /// Returns an error when the server cannot start or receive the text.
    pub fn problems(
        &self,
        documents: &DocumentManager,
        id: &DocumentId,
    ) -> Result<DocumentProblems, String> {
        self.prune(&documents.open_document_ids());
        let document = documents
            .server_document(id)
            .map_err(|error| error.to_string())?;
        let Some(document) = document else {
            return Ok(DocumentProblems {
                server: None,
                revision: 0,
                problems: Vec::new(),
            });
        };
        let Some((server, _)) = self.client_for(id, &document)? else {
            return Ok(DocumentProblems {
                server: None,
                revision: document.revision,
                problems: Vec::new(),
            });
        };
        self.collect_problems();
        let state = self.state();
        let open = state
            .running
            .values()
            .find_map(|running| running.documents.get(id));
        Ok(DocumentProblems {
            server: Some(server),
            revision: open.map_or(document.revision, |open| open.sent.revision),
            problems: open.map(|open| open.problems.clone()).unwrap_or_default(),
        })
    }

    /// Completion items from the document's server at its caret, as
    /// (label, kind, insert text, snippet, detail).
    ///
    /// # Errors
    /// Returns an error when the server fails or is late.
    pub fn completions(
        &self,
        documents: &DocumentManager,
        id: &DocumentId,
    ) -> Result<Vec<nulis_language::completion::CompletionItem>, String> {
        let Some(document) = documents
            .server_document(id)
            .map_err(|error| error.to_string())?
        else {
            return Ok(Vec::new());
        };
        let Some((_, client)) = self.client_for(id, &document)? else {
            return Ok(Vec::new());
        };
        let caret = document.text_offset(document.caret);
        let (line, character) = position_of(&document.text, caret, client.encoding());
        let result = client
            .request(
                "textDocument/completion",
                json!({ "textDocument": { "uri": file_uri(&document.path) },
                    "position": { "line": line, "character": character } }),
                COMPLETION_TIMEOUT,
            )
            .map_err(|error| error.to_string())?;
        let items = result
            .get("items")
            .and_then(Value::as_array)
            .or_else(|| result.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(items
            .iter()
            .take(MAX_COMPLETIONS)
            .filter_map(completion_item)
            .collect())
    }

    /// Formats the document with its server and applies the result as one
    /// undo step. `Ok(None)` when no server is set up for it.
    ///
    /// # Errors
    /// Returns an error when the server fails, is late, or cannot format,
    /// or the document changed meanwhile.
    pub fn format(
        &self,
        documents: &DocumentManager,
        id: &DocumentId,
        tab_size: u8,
        insert_spaces: bool,
    ) -> Result<Option<crate::editor::DocumentSummary>, String> {
        let Some(document) = documents
            .server_document(id)
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let Some((server, client)) = self.client_for(id, &document)? else {
            return Ok(None);
        };
        if client.capabilities()["documentFormattingProvider"].is_null()
            || client.capabilities()["documentFormattingProvider"] == false
        {
            return Err(format!("{server} cannot format documents"));
        }
        let edits = client
            .request(
                "textDocument/formatting",
                json!({ "textDocument": { "uri": file_uri(&document.path) },
                    "options": { "tabSize": tab_size, "insertSpaces": insert_spaces } }),
                FORMAT_TIMEOUT,
            )
            .map_err(|error| match error {
                LspError::Timeout(_) => format!("{server} took too long to format"),
                other => other.to_string(),
            })?;
        let formatted = apply_edits(&document.text, &edits, client.encoding());
        documents
            .replace_document_text(id, document.revision, &formatted)
            .map(Some)
            .map_err(|error| error.to_string())
    }

    /// Tests: start servers with a shorter deadline and forget back-off waits.
    #[cfg(test)]
    pub(crate) fn for_tests(&self, start_timeout: Duration) {
        *self
            .start_timeout
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = start_timeout;
        for failure in self.state().failures.values_mut() {
            failure.retry_after = None;
        }
    }

    /// Stops every server (when Nulis quits).
    pub fn shutdown_all(&self) {
        let mut state = self.state();
        for (_, running) in std::mem::take(&mut state.running) {
            running.client.shutdown(SHUTDOWN_TIMEOUT);
        }
    }
}

fn place_problem(document: &ServerDocument, encoding: PositionEncoding, item: &Value) -> Problem {
    let point = |position: &Value| {
        offset_of(
            &document.text,
            u32::try_from(position["line"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX),
            u32::try_from(position["character"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX),
            encoding,
        )
    };
    let start = point(&item["range"]["start"]);
    let end = point(&item["range"]["end"]).max(start);
    Problem {
        start_byte: document.document_offset(start),
        end_byte: document.document_offset(end),
        line: item["range"]["start"]["line"].as_u64().unwrap_or(0) + 1,
        severity: severity(&item["severity"]),
        message: item["message"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(2_000)
            .collect(),
        source: item["source"].as_str().map(str::to_owned),
    }
}

/// An LSP completion item as a Nulis one. `${1|a,b|}` choices become their
/// first choice, which the snippet expander understands.
fn completion_item(item: &Value) -> Option<nulis_language::completion::CompletionItem> {
    use nulis_language::completion::{CompletionItem, CompletionKind};
    let label = item["label"].as_str()?.to_owned();
    let insert = item["textEdit"]["newText"]
        .as_str()
        .or_else(|| item["insertText"].as_str())
        .unwrap_or(&label)
        .to_owned();
    let snippet = item["insertTextFormat"] == 2;
    let insert = if snippet {
        first_choices(&insert)
    } else {
        insert
    };
    // LSP kinds: 14 keyword, 15 snippet, 7/22/25 class/struct/type parameter.
    let kind = match item["kind"].as_u64() {
        Some(14) => CompletionKind::Keyword,
        Some(15) => CompletionKind::Snippet,
        Some(7 | 22 | 25 | 8 | 13) => CompletionKind::Type,
        _ => CompletionKind::Symbol,
    };
    Some(CompletionItem {
        label,
        kind,
        insert,
        snippet,
        detail: item["detail"]
            .as_str()
            .map(|detail| detail.chars().take(200).collect()),
    })
}

fn first_choices(snippet: &str) -> String {
    let mut out = String::with_capacity(snippet.len());
    let mut rest = snippet;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && tail[digits..].starts_with('|') {
            if let Some(end) = tail[digits + 1..].find("|}") {
                let choices = &tail[digits + 1..digits + 1 + end];
                let first = choices.split(',').next().unwrap_or_default();
                let _ = write!(out, "${{{}:{first}}}", &tail[..digits]);
                rest = &tail[digits + 1 + end + 2..];
                continue;
            }
        }
        out.push_str("${");
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// Applies LSP text edits (in any order, non-overlapping) to `text`.
fn apply_edits(text: &str, edits: &Value, encoding: PositionEncoding) -> String {
    let mut edits: Vec<(usize, usize, String)> = edits
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|edit| {
                    let point = |position: &Value| {
                        offset_of(
                            text,
                            u32::try_from(position["line"].as_u64().unwrap_or(0))
                                .unwrap_or(u32::MAX),
                            u32::try_from(position["character"].as_u64().unwrap_or(0))
                                .unwrap_or(u32::MAX),
                            encoding,
                        )
                    };
                    let start = point(&edit["range"]["start"]);
                    let end = point(&edit["range"]["end"]).max(start);
                    (
                        start,
                        end,
                        edit["newText"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    let mut result = text.to_owned();
    let mut limit = result.len();
    for (start, end, new_text) in edits {
        // Overlapping edits are dropped rather than corrupting the text.
        if end > limit {
            continue;
        }
        result.replace_range(start..end, &new_text);
        limit = start;
    }
    result
}

#[cfg(test)]
mod tests;
