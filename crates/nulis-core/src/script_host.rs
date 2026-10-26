//! What a running script may do to the editor: the host that turns its
//! requests into editor operations, within the permissions it was approved for.

use super::{
    is_script_command, CommandContext, CommandError, CommandRegistry, DocumentId, DocumentManager,
    ScriptEditDocumentArguments, ScriptReadWorkspaceArguments, ScriptWriteWorkspaceArguments,
    ViewportRequest, MAX_EDIT_BYTES, MAX_SCRIPT_MESSAGES, MAX_SCRIPT_MESSAGE_BYTES,
};
use nulis_automation::{
    ScriptDefinition, ScriptExecutionGuard, ScriptHost, ScriptHostCall, ScriptHostOperation,
    CURRENT_REVISION_ARGUMENT,
};
use nulis_filesystem::{fingerprint, save_atomic, WorkspaceService};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use std::sync::{Arc, Mutex};

pub(super) struct CoreScriptHost {
    pub(super) commands: CommandRegistry,
    pub(super) documents: Arc<DocumentManager>,
    pub(super) workspace: Arc<WorkspaceService>,
    pub(super) settings: Value,
    pub(super) context: CommandContext,
    pub(super) messages: Arc<Mutex<Vec<String>>>,
}

impl CoreScriptHost {
    fn document_id(&self) -> Result<DocumentId, String> {
        self.context
            .document_id
            .clone()
            .map(DocumentId)
            .ok_or_else(|| "operation requires a document context".into())
    }

    fn command_arguments(&self, mut arguments: Value) -> Result<Value, String> {
        let Some(object) = arguments.as_object_mut() else {
            return Ok(arguments);
        };
        if object.get("expectedRevision").and_then(Value::as_str) == Some(CURRENT_REVISION_ARGUMENT)
        {
            let revision = self
                .documents
                .summary(&self.document_id()?)
                .map_err(|error| error.to_string())?
                .revision;
            object.insert(
                "expectedRevision".into(),
                Value::String(revision.to_string()),
            );
        }
        Ok(arguments)
    }

    fn expected_revision(&self, value: &Value) -> Result<u64, String> {
        if value.as_str() == Some(CURRENT_REVISION_ARGUMENT) {
            return self
                .documents
                .summary(&self.document_id()?)
                .map(|document| document.revision)
                .map_err(|error| error.to_string());
        }
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|revision| revision.parse().ok()))
            .ok_or_else(|| {
                "expectedRevision must be a decimal revision or current-revision placeholder".into()
            })
    }
}

impl ScriptHost for CoreScriptHost {
    fn call(
        &mut self,
        request: ScriptHostCall,
        execution: &ScriptExecutionGuard,
    ) -> Result<Value, String> {
        execution.check(0).map_err(|error| error.to_string())?;
        match request.operation {
            ScriptHostOperation::ExecuteCommand { command } => {
                if !is_script_command(&command) {
                    return Err(format!("command is not permitted in scripts: {command}"));
                }
                let arguments = self.command_arguments(request.arguments)?;
                self.commands
                    .execute(&command, self.context.clone(), arguments)
                    .map_err(|error| error.to_string())
            }
            ScriptHostOperation::ReadDocument => {
                let document_id = self.document_id()?;
                let document = self
                    .documents
                    .summary(&document_id)
                    .map_err(|error| error.to_string())?;
                let viewport = self
                    .documents
                    .viewport(&ViewportRequest {
                        document_id,
                        start_byte: 0,
                        start_line: 1,
                        line_continuation: false,
                        max_bytes: 65_536,
                    })
                    .map_err(|error| error.to_string())?;
                Ok(json!({ "document": document, "viewport": viewport }))
            }
            ScriptHostOperation::EditDocument => {
                let arguments: ScriptEditDocumentArguments =
                    parse_host_arguments(request.arguments)?;
                let expected_revision = self.expected_revision(&arguments.expected_revision)?;
                let document = self
                    .documents
                    .replace(
                        &self.document_id()?,
                        expected_revision,
                        arguments.start_byte,
                        arguments.end_byte,
                        &arguments.text,
                    )
                    .map_err(|error| error.to_string())?;
                serde_json::to_value(document).map_err(|error| error.to_string())
            }
            ScriptHostOperation::ReadWorkspace => {
                let arguments: ScriptReadWorkspaceArguments =
                    parse_host_arguments(request.arguments)?;
                let maximum = arguments.max_entries.min(500);
                let directory = self
                    .workspace
                    .list(&arguments.path, maximum)
                    .map_err(|error| error.to_string())?;
                serde_json::to_value(directory).map_err(|error| error.to_string())
            }
            ScriptHostOperation::WriteWorkspace => {
                let arguments: ScriptWriteWorkspaceArguments =
                    parse_host_arguments(request.arguments)?;
                if arguments.contents.len() > MAX_EDIT_BYTES {
                    return Err(format!("workspace write exceeds {MAX_EDIT_BYTES} bytes"));
                }
                let path = self
                    .workspace
                    .resolve_file(&arguments.path)
                    .map_err(|error| error.to_string())?;
                let expected = fingerprint(&path).map_err(|error| error.to_string())?;
                let bytes = arguments.contents.into_bytes();
                save_atomic(&path, expected.as_ref(), |file| {
                    file.write_all(&bytes)?;
                    u64::try_from(bytes.len()).map_err(std::io::Error::other)
                })
                .map_err(|error| error.to_string())?;
                Ok(json!({ "path": arguments.path, "byteLen": bytes.len().to_string() }))
            }
            ScriptHostOperation::ReadSettings => Ok(self.settings.clone()),
            ScriptHostOperation::ShowMessage => {
                let message = request
                    .arguments
                    .as_str()
                    .ok_or_else(|| "message must be a string".to_owned())?;
                let mut messages = self
                    .messages
                    .lock()
                    .map_err(|_| "script message state is unavailable".to_owned())?;
                let total = messages
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(message.len());
                if messages.len() >= MAX_SCRIPT_MESSAGES || total > MAX_SCRIPT_MESSAGE_BYTES {
                    return Err("script message output limit exceeded".into());
                }
                messages.push(message.into());
                Ok(Value::Null)
            }
        }
    }
}

fn parse_host_arguments<T: for<'de> Deserialize<'de>>(arguments: Value) -> Result<T, String> {
    serde_json::from_value(arguments).map_err(|error| format!("invalid host arguments: {error}"))
}

pub(super) fn validate_script_commands(definition: &ScriptDefinition) -> Result<(), CommandError> {
    for command in &definition.requested_commands {
        if !is_script_command(command) {
            return Err(CommandError::Failed(format!(
                "command is not permitted in scripts: {command}"
            )));
        }
    }
    Ok(())
}
