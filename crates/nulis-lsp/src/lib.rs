//! One Language Server Protocol client for every language.
//!
//! A server is a separate program that Nulis starts with an argument vector
//! (never a shell string) and talks to over its standard input and output.
//! Messages are JSON-RPC with `Content-Length` framing. A reader thread
//! routes responses to the request waiting for them and queues
//! notifications; requests from the server get safe default answers.
//! Every request has a deadline; a request that misses it is cancelled.
//! Dropping the client stops the server.

use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fmt::Write as _,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use thiserror::Error;

/// Largest message read from a server.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
/// Most notifications kept unread; older ones are dropped.
const MAX_QUEUED_NOTIFICATIONS: usize = 1_024;

#[derive(Debug, Error)]
pub enum LspError {
    #[error("could not start the language server: {0}")]
    Start(String),
    #[error("the language server stopped")]
    Stopped,
    #[error("the language server did not answer {0} in time")]
    Timeout(String),
    #[error("the language server refused {method}: {message}")]
    Refused { method: String, message: String },
    #[error("malformed message from the language server: {0}")]
    Protocol(String),
    #[error("language server I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Writes one framed message.
///
/// # Errors
/// Returns an I/O error from the writer.
pub fn write_message(writer: &mut impl Write, message: &Value) -> Result<(), LspError> {
    let body =
        serde_json::to_vec(message).map_err(|error| LspError::Protocol(error.to_string()))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

/// Reads one framed message; `Ok(None)` at the end of the stream.
///
/// # Errors
/// Returns an error for a malformed header, an oversized or invalid body,
/// or a failed read.
pub fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>, LspError> {
    let mut length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        if let Some(value) = header
            .split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map(|(_, value)| value.trim())
        {
            length = Some(
                value
                    .parse()
                    .map_err(|_| LspError::Protocol(format!("bad length {value:?}")))?,
            );
        }
    }
    let length = length.ok_or_else(|| LspError::Protocol("no Content-Length".into()))?;
    if length > MAX_MESSAGE_BYTES {
        return Err(LspError::Protocol(format!("message of {length} bytes")));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| LspError::Protocol(error.to_string()))
}

/// How a server counts characters within a line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositionEncoding {
    Utf8,
    Utf16,
}

/// The LSP position (line, character) of a byte offset into `text`.
#[must_use]
pub fn position_of(text: &str, offset: usize, encoding: PositionEncoding) -> (u32, u32) {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = &before[line_start..];
    let character = match encoding {
        PositionEncoding::Utf8 => column.len(),
        PositionEncoding::Utf16 => column.encode_utf16().count(),
    };
    (
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(character).unwrap_or(u32::MAX),
    )
}

/// The byte offset into `text` of an LSP position; positions past a line's
/// end land at its end, and lines past the text at the text's end.
#[must_use]
pub fn offset_of(text: &str, line: u32, character: u32, encoding: PositionEncoding) -> usize {
    let mut line_start = 0;
    for _ in 0..line {
        match text[line_start..].find('\n') {
            Some(index) => line_start += index + 1,
            None => return text.len(),
        }
    }
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |index| line_start + index);
    let line_text = text[line_start..line_end].trim_end_matches('\r');
    let wanted = character as usize;
    let within = match encoding {
        PositionEncoding::Utf8 => floor_char_boundary(line_text, wanted.min(line_text.len())),
        PositionEncoding::Utf16 => {
            let mut units = 0;
            let mut bytes = line_text.len();
            for (index, unit) in line_text.char_indices() {
                if units >= wanted {
                    bytes = index;
                    break;
                }
                units += unit.len_utf16();
            }
            bytes
        }
    };
    line_start + within
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// A `file://` URI for an absolute path.
#[must_use]
pub fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from("file://");
    if !text.starts_with('/') {
        uri.push('/');
    }
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri
}

type Pending = Arc<Mutex<HashMap<i64, Sender<Result<Value, LspError>>>>>;

/// A running language server.
#[derive(Debug)]
pub struct LspClient {
    child: Mutex<Child>,
    writer: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    /// Set by the reader as soon as the server's output ends, which can be
    /// observed a moment before the operating system reports the exit.
    stopped: Arc<AtomicBool>,
    next_id: AtomicI64,
    notifications: Mutex<Receiver<Value>>,
    encoding: PositionEncoding,
    capabilities: Value,
}

impl LspClient {
    /// Starts `command` (program then arguments) in `root` and initializes
    /// it for that folder, asking for UTF-8 positions.
    ///
    /// # Errors
    /// Returns an error when the program cannot start or does not complete
    /// initialization within `timeout`.
    pub fn start(
        command: &[String],
        root: &Path,
        initialization_options: Option<&Value>,
        timeout: Duration,
    ) -> Result<Self, LspError> {
        let (program, arguments) = command
            .split_first()
            .ok_or_else(|| LspError::Start("no program given".into()))?;
        let mut child = Command::new(program)
            .args(arguments)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| LspError::Start(format!("{program}: {error}")))?;
        let stdin = child.stdin.take().ok_or(LspError::Stopped)?;
        let stdout = child.stdout.take().ok_or(LspError::Stopped)?;
        let writer = Arc::new(Mutex::new(stdin));
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let (notify, notifications) = mpsc::channel();
        spawn_reader(
            stdout,
            Arc::clone(&writer),
            Arc::clone(&pending),
            Arc::clone(&stopped),
            notify,
        );
        let mut client = Self {
            child: Mutex::new(child),
            writer,
            pending,
            stopped,
            next_id: AtomicI64::new(1),
            notifications: Mutex::new(notifications),
            encoding: PositionEncoding::Utf16,
            capabilities: Value::Null,
        };
        let root_uri = file_uri(root);
        let result = client.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": { "name": "Nulis", "version": env!("CARGO_PKG_VERSION") },
                "rootUri": root_uri,
                "workspaceFolders": [{ "uri": root_uri, "name": root.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default() }],
                "initializationOptions": initialization_options,
                "capabilities": {
                    "general": { "positionEncodings": ["utf-8", "utf-16"] },
                    "textDocument": {
                        "synchronization": { "dynamicRegistration": false },
                        "completion": { "completionItem": { "snippetSupport": true } },
                        "formatting": {},
                        "publishDiagnostics": {}
                    },
                    "workspace": { "configuration": true, "workspaceFolders": true }
                }
            }),
            timeout,
        )?;
        client.encoding = match result["capabilities"]["positionEncoding"].as_str() {
            Some("utf-8") => PositionEncoding::Utf8,
            _ => PositionEncoding::Utf16,
        };
        client.capabilities = result["capabilities"].clone();
        client.notify("initialized", json!({}))?;
        Ok(client)
    }

    #[must_use]
    pub fn encoding(&self) -> PositionEncoding {
        self.encoding
    }

    /// What the server said it can do (`initialize` result capabilities).
    #[must_use]
    pub fn capabilities(&self) -> &Value {
        &self.capabilities
    }

    /// Whether the server process is still running.
    pub fn is_running(&self) -> bool {
        if self.stopped.load(Ordering::Acquire) {
            return false;
        }
        self.child
            .lock()
            .is_ok_and(|mut child| matches!(child.try_wait(), Ok(None)))
    }

    /// Sends a request and waits up to `timeout` for its answer; a request
    /// that misses the deadline is cancelled.
    ///
    /// # Errors
    /// Returns an error when the server stopped, refused, or was late.
    pub fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LspError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (answer, receiver) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| LspError::Stopped)?
            .insert(id, answer);
        let mut message = json!({ "jsonrpc": "2.0", "id": id, "method": method });
        message["params"] = params;
        let sent = self.send(&message);
        if let Err(error) = sent {
            self.forget(id);
            return Err(error);
        }
        match receiver.recv_timeout(timeout) {
            Ok(result) => result.map_err(|error| match error {
                LspError::Refused { message, .. } => LspError::Refused {
                    method: method.to_owned(),
                    message,
                },
                other => other,
            }),
            Err(RecvTimeoutError::Timeout) => {
                self.forget(id);
                let _ = self.notify("$/cancelRequest", json!({ "id": id }));
                Err(LspError::Timeout(method.to_owned()))
            }
            Err(RecvTimeoutError::Disconnected) => Err(LspError::Stopped),
        }
    }

    /// Sends a notification.
    ///
    /// # Errors
    /// Returns an error when the server stopped.
    pub fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        {
            let mut message = json!({ "jsonrpc": "2.0", "method": method });
            message["params"] = params;
            self.send(&message)
        }
    }

    /// Notifications received since the last call, oldest first.
    pub fn take_notifications(&self) -> Vec<Value> {
        self.notifications
            .lock()
            .map(|receiver| receiver.try_iter().collect())
            .unwrap_or_default()
    }

    /// Asks the server to shut down, then stops it.
    pub fn shutdown(&self, timeout: Duration) {
        let _ = self.request("shutdown", Value::Null, timeout);
        let _ = self.notify("exit", Value::Null);
        if let Ok(mut child) = self.child.lock() {
            for _ in 0..20 {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn send(&self, message: &Value) -> Result<(), LspError> {
        let mut writer = self.writer.lock().map_err(|_| LspError::Stopped)?;
        write_message(&mut *writer, message).map_err(|_| LspError::Stopped)
    }

    fn forget(&self, id: i64) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&id);
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Reads messages until the server's output ends: answers go to waiting
/// requests, notifications to the queue, and server requests get defaults.
fn spawn_reader(
    stdout: std::process::ChildStdout,
    writer: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    stopped: Arc<AtomicBool>,
    notifications: Sender<Value>,
) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut queued = 0_usize;
        while let Ok(Some(message)) = read_message(&mut reader) {
            let id = message.get("id").cloned();
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .map(str::to_owned);
            match (id, method) {
                // An answer to one of our requests.
                (Some(id), None) => {
                    let Some(id) = id.as_i64() else { continue };
                    let waiting = pending
                        .lock()
                        .ok()
                        .and_then(|mut pending| pending.remove(&id));
                    if let Some(waiting) = waiting {
                        let result = match message.get("error") {
                            Some(error) => Err(LspError::Refused {
                                method: String::new(),
                                message: error["message"].as_str().unwrap_or("error").to_owned(),
                            }),
                            None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = waiting.send(result);
                    }
                }
                // A request from the server: answer what it needs to go on.
                (Some(id), Some(method)) => {
                    let result = match method.as_str() {
                        "workspace/configuration" => {
                            let count = message["params"]["items"].as_array().map_or(0, Vec::len);
                            Value::Array(vec![Value::Null; count])
                        }
                        _ => Value::Null,
                    };
                    if let Ok(mut writer) = writer.lock() {
                        let _ = write_message(
                            &mut *writer,
                            &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                        );
                    }
                }
                // A notification.
                (None, Some(_)) => {
                    queued += 1;
                    if queued <= MAX_QUEUED_NOTIFICATIONS
                        || message["method"] == "textDocument/publishDiagnostics"
                    {
                        let _ = notifications.send(message);
                    }
                }
                (None, None) => {}
            }
        }
        // The server stopped: wake every waiting request.
        stopped.store(true, Ordering::Release);
        if let Ok(mut pending) = pending.lock() {
            for (_, waiting) in pending.drain() {
                let _ = waiting.send(Err(LspError::Stopped));
            }
        }
    });
}

#[cfg(test)]
mod tests;
