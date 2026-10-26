//! Bounded cross-platform pseudoterminal sessions.

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};
use thiserror::Error;

pub const MAX_TERMINAL_SESSIONS: usize = 16;
pub const MAX_TERMINAL_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_TERMINAL_READ_BYTES: usize = 64 * 1024;
pub const MAX_TERMINAL_BUFFER_BYTES: usize = 1024 * 1024;
const MAX_TERMINAL_ARGUMENTS: usize = 128;
const MAX_TERMINAL_ARGUMENT_BYTES: usize = 32 * 1024;
const MAX_TERMINAL_ENVIRONMENT: usize = 64;
const MAX_TERMINAL_ENVIRONMENT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TerminalId(pub String);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            columns: 80,
            rows: 24,
        }
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalSpawnRequest {
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub size: TerminalSize,
}

impl std::fmt::Debug for TerminalSpawnRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TerminalSpawnRequest")
            .field("program", &self.program)
            .field("argument_count", &self.arguments.len())
            .field("cwd", &self.cwd)
            .field(
                "environment_keys",
                &self.environment.keys().collect::<Vec<_>>(),
            )
            .field("size", &self.size)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSummary {
    pub terminal_id: TerminalId,
    pub title: String,
    pub size: TerminalSize,
    pub running: bool,
    pub exit_code: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutput {
    pub terminal_id: TerminalId,
    pub bytes: Vec<u8>,
    #[serde(with = "decimal_u64")]
    pub start_offset: u64,
    #[serde(with = "decimal_u64")]
    pub next_offset: u64,
    pub dropped: bool,
    pub running: bool,
    pub exit_code: Option<u32>,
}

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("unknown terminal {0:?}")]
    UnknownTerminal(TerminalId),
    #[error("terminal session limit of {MAX_TERMINAL_SESSIONS} reached")]
    SessionLimit,
    #[error("terminal dimensions must be between 2 and 1,000 cells")]
    InvalidSize,
    #[error("terminal program cannot be empty")]
    EmptyProgram,
    #[error("terminal argument count exceeds {MAX_TERMINAL_ARGUMENTS}")]
    TooManyArguments,
    #[error("terminal argument data exceeds {MAX_TERMINAL_ARGUMENT_BYTES} bytes")]
    ArgumentsTooLarge,
    #[error("terminal environment exceeds its bounded limits")]
    EnvironmentTooLarge,
    #[error("terminal input exceeds {MAX_TERMINAL_INPUT_BYTES} bytes")]
    InputTooLarge,
    #[error("terminal read limit must be in 1..={MAX_TERMINAL_READ_BYTES}")]
    InvalidReadLimit,
    #[error("terminal output cursor {cursor} exceeds current output end {end}")]
    InvalidCursor { cursor: u64, end: u64 },
    #[error("terminal state is unavailable")]
    StateUnavailable,
    #[error("terminal operation failed: {0}")]
    Operation(String),
}

#[derive(Debug)]
struct OutputBuffer {
    bytes: VecDeque<u8>,
    start_offset: u64,
    end_offset: u64,
}

impl Default for OutputBuffer {
    fn default() -> Self {
        Self {
            bytes: VecDeque::with_capacity(MAX_TERMINAL_BUFFER_BYTES),
            start_offset: 0,
            end_offset: 0,
        }
    }
}

impl OutputBuffer {
    fn append(&mut self, bytes: &[u8]) {
        self.bytes.extend(bytes);
        self.end_offset = self.end_offset.saturating_add(bytes.len() as u64);
        while self.bytes.len() > MAX_TERMINAL_BUFFER_BYTES {
            self.bytes.pop_front();
            self.start_offset = self.start_offset.saturating_add(1);
        }
    }

    fn read(
        &self,
        cursor: u64,
        maximum: usize,
    ) -> Result<(Vec<u8>, u64, u64, bool), TerminalError> {
        if cursor > self.end_offset {
            return Err(TerminalError::InvalidCursor {
                cursor,
                end: self.end_offset,
            });
        }
        let dropped = cursor < self.start_offset;
        let actual = cursor.max(self.start_offset);
        let relative = usize::try_from(actual - self.start_offset)
            .map_err(|error| TerminalError::Operation(error.to_string()))?;
        let count = self.bytes.len().saturating_sub(relative).min(maximum);
        let bytes = self
            .bytes
            .iter()
            .skip(relative)
            .take(count)
            .copied()
            .collect();
        let next = actual.saturating_add(count as u64);
        Ok((bytes, actual, next, dropped))
    }
}

struct TerminalSession {
    id: TerminalId,
    title: String,
    size: TerminalSize,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output: Arc<Mutex<OutputBuffer>>,
    reader_ended: Arc<AtomicBool>,
    exit_code: Option<u32>,
}

impl std::fmt::Debug for TerminalSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TerminalSession")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("size", &self.size)
            .field("reader_ended", &self.reader_ended)
            .field("exit_code", &self.exit_code)
            .finish_non_exhaustive()
    }
}

impl TerminalSession {
    fn refresh_exit(&mut self) -> Result<(), TerminalError> {
        if self.exit_code.is_some() {
            return Ok(());
        }
        if let Some(status) = self
            .child
            .try_wait()
            .map_err(|error| TerminalError::Operation(error.to_string()))?
        {
            self.exit_code = Some(status.exit_code());
        }
        Ok(())
    }

    fn summary(&self) -> TerminalSummary {
        TerminalSummary {
            terminal_id: self.id.clone(),
            title: self.title.clone(),
            size: self.size,
            running: self.exit_code.is_none(),
            exit_code: self.exit_code,
        }
    }
}

#[derive(Debug, Default)]
pub struct TerminalManager {
    next_id: AtomicU64,
    sessions: Mutex<BTreeMap<TerminalId, TerminalSession>>,
}

impl TerminalManager {
    /// Spawns one program directly in a native PTY. Arguments are never joined
    /// into a shell command. When `program` is absent, the platform's default
    /// interactive shell is selected by `portable-pty`.
    ///
    /// # Errors
    /// Returns an error for invalid limits, PTY creation, or process spawning.
    pub fn spawn(&self, request: &TerminalSpawnRequest) -> Result<TerminalSummary, TerminalError> {
        validate_spawn_request(request)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| TerminalError::StateUnavailable)?;
        if sessions.len() >= MAX_TERMINAL_SESSIONS {
            return Err(TerminalError::SessionLimit);
        }
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(pty_size(request.size))
            .map_err(|error| TerminalError::Operation(error.to_string()))?;
        let mut command = request
            .program
            .as_ref()
            .map_or_else(CommandBuilder::new_default_prog, |program| {
                CommandBuilder::new(program)
            });
        command.args(&request.arguments);
        if let Some(cwd) = &request.cwd {
            command.cwd(cwd);
        }
        for (key, value) in &request.environment {
            command.env(key, value);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| TerminalError::Operation(error.to_string()))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| TerminalError::Operation(error.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| TerminalError::Operation(error.to_string()))?;
        let output = Arc::new(Mutex::new(OutputBuffer::default()));
        let reader_output = Arc::clone(&output);
        let reader_ended = Arc::new(AtomicBool::new(false));
        let ended = Arc::clone(&reader_ended);
        thread::Builder::new()
            .name("nulis-terminal-reader".into())
            .spawn(move || {
                let mut bytes = [0_u8; 8 * 1024];
                loop {
                    match reader.read(&mut bytes) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => {
                            let Ok(mut output) = reader_output.lock() else {
                                break;
                            };
                            output.append(&bytes[..count]);
                        }
                    }
                }
                ended.store(true, Ordering::Release);
            })
            .map_err(|error| TerminalError::Operation(error.to_string()))?;
        let number = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let id = TerminalId(format!("terminal-{number}"));
        let title = request.program.as_ref().map_or_else(
            || "Shell".into(),
            |program| {
                std::path::Path::new(program).file_name().map_or_else(
                    || program.clone(),
                    |name| name.to_string_lossy().into_owned(),
                )
            },
        );
        let session = TerminalSession {
            id: id.clone(),
            title,
            size: request.size,
            master: pair.master,
            child,
            writer,
            output,
            reader_ended,
            exit_code: None,
        };
        let summary = session.summary();
        sessions.insert(id, session);
        Ok(summary)
    }

    /// Returns summaries for all live or completed sessions.
    ///
    /// # Errors
    /// Returns an error when state or process polling is unavailable.
    pub fn list(&self) -> Result<Vec<TerminalSummary>, TerminalError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| TerminalError::StateUnavailable)?;
        sessions
            .values_mut()
            .map(|session| {
                session.refresh_exit()?;
                Ok(session.summary())
            })
            .collect()
    }

    /// Writes a bounded byte payload to a terminal.
    ///
    /// # Errors
    /// Returns an error for an unknown session, oversized input, or I/O failure.
    pub fn write(&self, id: &TerminalId, bytes: &[u8]) -> Result<(), TerminalError> {
        if bytes.len() > MAX_TERMINAL_INPUT_BYTES {
            return Err(TerminalError::InputTooLarge);
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| TerminalError::StateUnavailable)?;
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| TerminalError::UnknownTerminal(id.clone()))?;
        session
            .writer
            .write_all(bytes)
            .and_then(|()| session.writer.flush())
            .map_err(|error| TerminalError::Operation(error.to_string()))
    }

    /// Reads a bounded output page from an absolute stream cursor.
    ///
    /// # Errors
    /// Returns an error for an invalid cursor/limit or unavailable session.
    pub fn read(
        &self,
        id: &TerminalId,
        cursor: u64,
        maximum: usize,
    ) -> Result<TerminalOutput, TerminalError> {
        if maximum == 0 || maximum > MAX_TERMINAL_READ_BYTES {
            return Err(TerminalError::InvalidReadLimit);
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| TerminalError::StateUnavailable)?;
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| TerminalError::UnknownTerminal(id.clone()))?;
        session.refresh_exit()?;
        let (bytes, start_offset, next_offset, dropped) = session
            .output
            .lock()
            .map_err(|_| TerminalError::StateUnavailable)?
            .read(cursor, maximum)?;
        Ok(TerminalOutput {
            terminal_id: id.clone(),
            bytes,
            start_offset,
            next_offset,
            dropped,
            running: session.exit_code.is_none(),
            exit_code: session.exit_code,
        })
    }

    /// Resizes an existing PTY.
    ///
    /// # Errors
    /// Returns an error for invalid dimensions, unknown sessions, or PTY failure.
    pub fn resize(&self, id: &TerminalId, size: TerminalSize) -> Result<(), TerminalError> {
        validate_size(size)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| TerminalError::StateUnavailable)?;
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| TerminalError::UnknownTerminal(id.clone()))?;
        session
            .master
            .resize(pty_size(size))
            .map_err(|error| TerminalError::Operation(error.to_string()))?;
        session.size = size;
        Ok(())
    }

    /// Terminates and removes a terminal session.
    ///
    /// # Errors
    /// Returns an error for an unknown session or process failure.
    pub fn close(&self, id: &TerminalId) -> Result<(), TerminalError> {
        let mut session = self
            .sessions
            .lock()
            .map_err(|_| TerminalError::StateUnavailable)?
            .remove(id)
            .ok_or_else(|| TerminalError::UnknownTerminal(id.clone()))?;
        if session.exit_code.is_none() {
            session
                .child
                .kill()
                .map_err(|error| TerminalError::Operation(error.to_string()))?;
        }
        Ok(())
    }
}

impl Drop for TerminalManager {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            for session in sessions.values_mut() {
                if session.exit_code.is_none() {
                    let _ = session.child.kill();
                }
            }
        }
    }
}

fn validate_spawn_request(request: &TerminalSpawnRequest) -> Result<(), TerminalError> {
    validate_size(request.size)?;
    if request.program.as_deref().is_some_and(str::is_empty) {
        return Err(TerminalError::EmptyProgram);
    }
    if request.arguments.len() > MAX_TERMINAL_ARGUMENTS {
        return Err(TerminalError::TooManyArguments);
    }
    let argument_bytes = request.arguments.iter().map(String::len).sum::<usize>();
    if argument_bytes > MAX_TERMINAL_ARGUMENT_BYTES {
        return Err(TerminalError::ArgumentsTooLarge);
    }
    let environment_bytes = request
        .environment
        .iter()
        .map(|(key, value)| key.len().saturating_add(value.len()))
        .sum::<usize>();
    if request.environment.len() > MAX_TERMINAL_ENVIRONMENT
        || environment_bytes > MAX_TERMINAL_ENVIRONMENT_BYTES
        || request
            .environment
            .keys()
            .any(|key| key.is_empty() || key.contains('='))
    {
        return Err(TerminalError::EnvironmentTooLarge);
    }
    Ok(())
}

fn validate_size(size: TerminalSize) -> Result<(), TerminalError> {
    if (2..=1_000).contains(&size.columns) && (2..=1_000).contains(&size.rows) {
        Ok(())
    } else {
        Err(TerminalError::InvalidSize)
    }
}

const fn pty_size(size: TerminalSize) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.columns,
        pixel_width: 0,
        pixel_height: 0,
    }
}

mod decimal_u64 {
    use serde::Serializer;

    #[allow(clippy::trivially_copy_pass_by_ref)] // Serde's `with` adapter requires this signature.
    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OutputBuffer, TerminalManager, TerminalSize, TerminalSpawnRequest,
        MAX_TERMINAL_BUFFER_BYTES,
    };
    use std::collections::BTreeMap;
    #[cfg(unix)]
    use std::{thread, time::Duration};

    #[test]
    fn output_buffer_evicts_by_bytes_and_reports_dropped_cursors() {
        let mut output = OutputBuffer::default();
        output.append(&vec![b'a'; MAX_TERMINAL_BUFFER_BYTES]);
        output.append(b"tail");
        let (bytes, start, next, dropped) = output.read(0, 8).expect("read");
        assert!(dropped);
        assert_eq!(bytes, b"aaaaaaaa");
        assert_eq!(start, 4);
        assert_eq!(next, 12);

        let tail_cursor = u64::try_from(MAX_TERMINAL_BUFFER_BYTES).expect("fits");
        let (bytes, start, next, dropped) = output.read(tail_cursor, 8).expect("read tail");
        assert!(!dropped);
        assert_eq!(bytes, b"tail");
        assert_eq!(start, tail_cursor);
        assert_eq!(next, tail_cursor + 4);
    }

    #[cfg(unix)]
    #[test]
    fn spawned_program_output_is_bounded_and_cursor_addressed() {
        let manager = TerminalManager::default();
        let terminal = manager
            .spawn(&TerminalSpawnRequest {
                program: Some("/bin/sh".into()),
                arguments: vec!["-c".into(), "printf nulis-terminal".into()],
                cwd: None,
                environment: BTreeMap::new(),
                size: TerminalSize::default(),
            })
            .expect("spawn");
        let mut output = Vec::new();
        let mut cursor = 0;
        for _ in 0..100 {
            let page = manager
                .read(&terminal.terminal_id, cursor, 1024)
                .expect("read");
            cursor = page.next_offset;
            output.extend(page.bytes);
            if !page.running && output.ends_with(b"nulis-terminal") {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(String::from_utf8_lossy(&output).contains("nulis-terminal"));
        manager.close(&terminal.terminal_id).expect("close");
    }

    #[cfg(unix)]
    #[test]
    fn a_shell_takes_input_after_a_resize_and_stays_the_same_session() {
        let manager = TerminalManager::default();
        let terminal = manager
            .spawn(&TerminalSpawnRequest {
                program: Some("/bin/sh".into()),
                arguments: Vec::new(),
                cwd: None,
                environment: BTreeMap::new(),
                size: TerminalSize {
                    columns: 40,
                    rows: 10,
                },
            })
            .expect("spawn");
        // The PTY echoes typed bytes, so a needle that also appears in the command would
        // be satisfied by the echo; the quotes keep the command text from containing it.
        let id = terminal.terminal_id.clone();
        let mut cursor = 0;
        let mut seen = String::new();
        let mut running = true;
        manager
            .write(&id, b"echo nulis\"\"first\n")
            .expect("first write");
        for _ in 0..200 {
            let page = manager.read(&id, cursor, 1024).expect("read");
            cursor = page.next_offset;
            running = page.running;
            seen.push_str(&String::from_utf8_lossy(&page.bytes));
            if seen.contains("nulisfirst") {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            running,
            "the shell exited before the resize; output so far: {seen:?}"
        );
        assert!(
            seen.contains("nulisfirst"),
            "the shell answered no first command; output: {seen:?}"
        );

        manager
            .resize(
                &id,
                TerminalSize {
                    columns: 90,
                    rows: 24,
                },
            )
            .expect("resize");
        manager
            .write(&id, b"echo nulis\"\"after\n")
            .expect("second write");
        for _ in 0..200 {
            let page = manager.read(&id, cursor, 1024).expect("read");
            cursor = page.next_offset;
            running = page.running;
            seen.push_str(&String::from_utf8_lossy(&page.bytes));
            if seen.contains("nulisafter") {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            seen.contains("nulisafter"),
            "the shell took no input after the resize; output: {seen:?}"
        );
        assert!(running, "the shell exited on the resize; output: {seen:?}");

        // The resize kept the session: one session, at the size it was given.
        let sessions = manager.list().expect("list");
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].size,
            TerminalSize {
                columns: 90,
                rows: 24
            }
        );
        assert!(sessions[0].running);

        manager.close(&id).expect("close");
        let closed = manager.list().expect("closed list");
        assert_eq!(closed.len(), 0, "closing left the session listed");
    }

    #[test]
    fn invalid_spawn_limits_fail_before_process_creation() {
        let manager = TerminalManager::default();
        let result = manager.spawn(&TerminalSpawnRequest {
            program: Some(String::new()),
            arguments: Vec::new(),
            cwd: None,
            environment: BTreeMap::new(),
            size: TerminalSize::default(),
        });
        assert!(result.is_err());
    }
}
