use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use thiserror::Error;

use nulis_automation::{run_macro, AutomationError, MacroDefinition, MacroRunReport};

pub type CommandResult = Result<Value, CommandError>;
type Handler = Arc<dyn Fn(CommandContext, Value) -> CommandResult + Send + Sync>;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandContext {
    pub window_id: Option<String>,
    pub document_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandDescriptor {
    pub id: String,
    pub title: String,
    pub category: String,
}

impl CommandDescriptor {
    /// Creates metadata after validating its stable command identifier.
    ///
    /// # Errors
    /// Returns [`CommandError::InvalidId`] for an invalid identifier.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        category: impl Into<String>,
    ) -> Result<Self, CommandError> {
        let id = id.into();
        validate_id(&id)?;
        Ok(Self {
            id,
            title: title.into(),
            category: category.into(),
        })
    }
}

#[derive(Clone, Default)]
pub struct CommandRegistry {
    entries: BTreeMap<String, (CommandDescriptor, Handler)>,
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandRegistry")
            .field("command_ids", &self.entries.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl CommandRegistry {
    /// Registers one command. Duplicate IDs are rejected rather than shadowed.
    ///
    /// # Errors
    /// Returns [`CommandError::Duplicate`] when the ID is already registered.
    pub fn register<F>(
        &mut self,
        descriptor: CommandDescriptor,
        handler: F,
    ) -> Result<(), CommandError>
    where
        F: Fn(CommandContext, Value) -> CommandResult + Send + Sync + 'static,
    {
        if self.entries.contains_key(&descriptor.id) {
            return Err(CommandError::Duplicate(descriptor.id));
        }
        self.entries
            .insert(descriptor.id.clone(), (descriptor, Arc::new(handler)));
        Ok(())
    }

    #[must_use]
    pub fn descriptors(&self) -> Vec<CommandDescriptor> {
        self.entries
            .values()
            .map(|(descriptor, _)| descriptor.clone())
            .collect()
    }

    /// Invokes a command with JSON-compatible arguments.
    ///
    /// # Errors
    /// Returns [`CommandError::Unknown`] if no command is registered for `id`,
    /// or propagates the handler's error.
    pub fn execute(&self, id: &str, context: CommandContext, arguments: Value) -> CommandResult {
        let (_, handler) = self
            .entries
            .get(id)
            .ok_or_else(|| CommandError::Unknown(id.to_owned()))?;
        handler(context, arguments)
    }

    /// Executes a bounded recorded sequence through the same registered
    /// handlers as direct UI actions.
    ///
    /// # Errors
    /// Returns a macro validation, cancellation, or indexed command error.
    pub fn execute_macro<Cancel>(
        &self,
        definition: &MacroDefinition,
        context: &CommandContext,
        is_cancelled: Cancel,
    ) -> Result<MacroRunReport, AutomationError>
    where
        Cancel: FnMut() -> bool,
    {
        run_macro(
            definition,
            |action| {
                self.execute(&action.command, context.clone(), action.arguments.clone())
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            },
            is_cancelled,
        )
    }
}

fn validate_id(id: &str) -> Result<(), CommandError> {
    let valid = !id.is_empty()
        && id.contains('.')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(CommandError::InvalidId(id.into()))
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum CommandError {
    #[error("invalid command ID: {0}")]
    InvalidId(String),
    #[error("command is already registered: {0}")]
    Duplicate(String),
    #[error("unknown command: {0}")]
    Unknown(String),
    #[error("command is unavailable: {0}")]
    Unavailable(String),
    #[error("command failed: {0}")]
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::{CommandContext, CommandDescriptor, CommandError, CommandRegistry};
    use nulis_automation::{MacroDefinition, RecordedCommand, AUTOMATION_SCHEMA_VERSION};
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    #[test]
    fn duplicate_commands_cannot_shadow_handlers() {
        let mut registry = CommandRegistry::default();
        let descriptor = CommandDescriptor::new("test.run", "Run", "Test").expect("valid");
        registry
            .register(descriptor.clone(), |_, _| Ok(Value::Null))
            .expect("first registration");
        assert_eq!(
            registry.register(descriptor, |_, _| Ok(Value::Null)),
            Err(CommandError::Duplicate("test.run".into()))
        );
    }

    #[test]
    fn handler_receives_context_and_arguments() {
        let mut registry = CommandRegistry::default();
        registry
            .register(
                CommandDescriptor::new("test.echo", "Echo", "Test").expect("valid"),
                |context, arguments| Ok(json!({ "window": context.window_id, "value": arguments })),
            )
            .expect("registration");
        let result = registry
            .execute(
                "test.echo",
                CommandContext {
                    window_id: Some("main".into()),
                    document_id: None,
                },
                json!(42),
            )
            .expect("execution");
        assert_eq!(result, json!({ "window": "main", "value": 42 }));
    }

    #[test]
    fn ids_are_namespaced_and_ascii_safe() {
        assert!(CommandDescriptor::new("open", "Open", "File").is_err());
        assert!(CommandDescriptor::new("file.open!", "Open", "File").is_err());
        assert!(CommandDescriptor::new("file.open", "Open", "File").is_ok());
    }

    #[test]
    fn macros_use_registered_handlers_in_order_and_stop_on_failure() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut registry = CommandRegistry::default();
        for id in ["test.first", "test.second"] {
            let calls = Arc::clone(&calls);
            registry
                .register(
                    CommandDescriptor::new(id, id, "Test").expect("valid"),
                    move |context, arguments| {
                        calls
                            .lock()
                            .expect("calls")
                            .push((context.document_id, arguments.as_u64().expect("number")));
                        Ok(Value::Null)
                    },
                )
                .expect("registration");
        }
        let definition = MacroDefinition {
            schema_version: AUTOMATION_SCHEMA_VERSION,
            id: "user.ordered".into(),
            name: "Ordered".into(),
            actions: vec![
                RecordedCommand {
                    command: "test.first".into(),
                    arguments: json!(1),
                },
                RecordedCommand {
                    command: "test.missing".into(),
                    arguments: json!(2),
                },
                RecordedCommand {
                    command: "test.second".into(),
                    arguments: json!(3),
                },
            ],
        };
        let failure = registry.execute_macro(
            &definition,
            &CommandContext {
                window_id: None,
                document_id: Some("document-1".into()),
            },
            || false,
        );
        assert!(matches!(
            failure,
            Err(nulis_automation::AutomationError::CommandFailed { index: 1, .. })
        ));
        assert_eq!(
            *calls.lock().expect("calls"),
            vec![(Some("document-1".into()), 1)]
        );
    }
}
