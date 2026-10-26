//! Languages and their capabilities: the catalog, user and plugin file
//! associations, which features each document has, and View As.

use super::{
    AssociationClaim, CapabilitySwitch, DocumentCapabilities, DocumentId, DocumentManager,
    EditorError, LanguageCapabilities, LanguageConflict, LanguageDescriptor, PluginLanguageSync,
};
use nulis_language::{CapabilityKind, CapabilityRegistry, Provider};
use nulis_syntax::{AssociationSource, LanguageId, LanguageRegistry};
use std::collections::BTreeMap;

impl DocumentManager {
    /// The language catalog. Readers cannot leave it inconsistent, so a lock
    /// poisoned by a panic elsewhere is still safe to read.
    pub(super) fn languages(&self) -> std::sync::RwLockReadGuard<'_, LanguageRegistry> {
        self.languages
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Replaces the user's file associations (`files.associations`) for
    /// documents opened from now on. Returns the patterns that were left out
    /// because the pattern or the language is not valid.
    pub fn set_user_associations<'a>(
        &self,
        entries: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Vec<String> {
        self.languages
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_user_associations(entries)
    }

    /// Rebuilds every plugin-contributed language, association, and provider.
    /// Returns one message per contribution that could not be applied; a bad
    /// plugin never stops the others, and a document whose language vanished
    /// stays readable as Plain Text.
    pub fn apply_plugin_languages(&self, sync: PluginLanguageSync) -> Vec<String> {
        let mut errors = sync.errors;
        let mut providers = Vec::new();
        {
            let mut registry = self
                .languages
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            registry.clear_plugin_languages();
            for language in sync.languages {
                let crate::plugin_languages::PluginLanguage {
                    plugin_id,
                    module,
                    association_only,
                    providers: language_providers,
                } = language;
                let id = module.descriptor().id.clone();
                if registry.contains(&id) {
                    if association_only {
                        let descriptor = module.descriptor();
                        match registry.add_plugin_associations(
                            &plugin_id,
                            &id,
                            &descriptor.extensions,
                            &descriptor.filenames,
                        ) {
                            Ok(()) => providers.extend(language_providers),
                            Err(error) => errors.push(format!("{plugin_id}: {error}")),
                        }
                    } else {
                        errors.push(format!(
                            "plugin {plugin_id} cannot redefine built-in language {}",
                            id.as_str()
                        ));
                    }
                } else {
                    match registry.register(module, AssociationSource::Plugin(plugin_id.clone())) {
                        Ok(()) => providers.extend(language_providers),
                        Err(error) => errors.push(format!("{plugin_id}: {error}")),
                    }
                }
            }
        }
        {
            let mut capabilities = self
                .capabilities
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            capabilities.remove_all_plugin_providers();
            for (info, provider) in providers {
                if let Err(error) = capabilities.register(info, provider) {
                    errors.push(error.to_string());
                }
            }
        }
        if let Ok(mut sessions) = self.sessions.lock() {
            for session in sessions.values_mut() {
                self.refresh_language_state(session);
            }
        }
        errors
    }

    /// File patterns claimed by more than one language, in pattern order.
    /// Built-in claims come first, then plugins by ID, which is the order the
    /// registry resolves them in.
    #[must_use]
    pub fn language_conflicts(&self) -> Vec<LanguageConflict> {
        let registry = self.languages();
        let names = registry
            .descriptors()
            .into_iter()
            .map(|descriptor| (descriptor.id, descriptor.display_name))
            .collect::<BTreeMap<_, _>>();
        registry
            .conflicts()
            .into_iter()
            .map(|conflict| LanguageConflict {
                pattern: conflict.matcher.pattern(),
                claims: conflict
                    .claims
                    .into_iter()
                    .map(|(language, source)| AssociationClaim {
                        display_name: names
                            .get(&language)
                            .cloned()
                            .unwrap_or_else(|| language.as_str().to_owned()),
                        language: language.as_str().to_owned(),
                        source: match source {
                            AssociationSource::Builtin => "builtin".into(),
                            AssociationSource::Plugin(plugin) => plugin,
                            AssociationSource::User => "user".into(),
                        },
                    })
                    .collect(),
            })
            .collect()
    }

    pub(super) fn capability_registry(&self) -> std::sync::RwLockReadGuard<'_, CapabilityRegistry> {
        self.capabilities
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// What a language offers right now, from the capability registry.
    pub(super) fn document_capabilities(&self, language: &LanguageId) -> DocumentCapabilities {
        let registry = self.capability_registry();
        let label = |kind| {
            registry
                .resolve(language, kind)
                .map(|resolved| resolved.info.label)
        };
        let formatter = registry.resolve(language, CapabilityKind::Format);
        let capabilities = DocumentCapabilities {
            validate: label(CapabilityKind::Diagnostics),
            format: formatter.as_ref().map(|resolved| resolved.info.label.clone()),
            compact: label(CapabilityKind::Compact),
            format_may_lose_data: formatter.is_some_and(|resolved| {
                matches!(resolved.provider, Provider::Formatter(formatter) if formatter.may_lose_data())
            }),
            preview: registry
                .resolve(language, CapabilityKind::Preview)
                .and_then(|resolved| match resolved.provider {
                    Provider::Preview(preview) => Some(preview.kind().to_owned()),
                    _ => None,
                }),
            completion: None,
            language_server: None,
        };
        drop(registry);
        let language_server = self
            .language_servers
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(language.as_str())
            .cloned();
        DocumentCapabilities {
            completion: self.completion_mode(language),
            // Beautify falls back to the language server's formatting.
            format: capabilities
                .format
                .clone()
                .or_else(|| language_server.clone()),
            language_server,
            ..capabilities
        }
    }

    /// Switches capabilities off per language (`"[json]": { "disabledCapabilities":
    /// ["format"] }` in `settings.json`); everything not listed is on. Open
    /// documents take the change at once; their content is untouched.
    pub fn set_disabled_capabilities(
        &self,
        disabled: impl IntoIterator<Item = (LanguageId, CapabilityKind)>,
    ) {
        self.capabilities
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace_disabled(disabled);
        if let Ok(mut sessions) = self.sessions.lock() {
            for session in sessions.values_mut() {
                self.refresh_language_state(session);
            }
        }
    }

    /// Every language that has a switchable capability, with each capability's
    /// provider label and whether it is on, for Settings.
    #[must_use]
    pub fn language_capabilities(&self) -> Vec<LanguageCapabilities> {
        let registry = self.capability_registry();
        let providers = registry.providers();
        self.language_descriptors()
            .into_iter()
            .filter_map(|descriptor| {
                let language = LanguageId::new(descriptor.id.clone()).ok()?;
                let mut capabilities: Vec<CapabilitySwitch> = Vec::new();
                for (info, _) in providers
                    .iter()
                    .filter(|(info, _)| info.languages.contains(&language))
                {
                    if capabilities.iter().any(|item| item.kind == info.kind) {
                        continue;
                    }
                    capabilities.push(CapabilitySwitch {
                        kind: info.kind,
                        label: info.label.clone(),
                        enabled: registry.is_capability_enabled(&language, info.kind),
                    });
                }
                (!capabilities.is_empty()).then_some(LanguageCapabilities {
                    id: descriptor.id,
                    display_name: descriptor.display_name,
                    capabilities,
                })
            })
            .collect()
    }

    #[must_use]
    pub fn language_descriptors(&self) -> Vec<LanguageDescriptor> {
        self.languages()
            .descriptors()
            .into_iter()
            .map(|descriptor| LanguageDescriptor {
                id: descriptor.id.as_str().to_owned(),
                display_name: descriptor.display_name,
            })
            .collect()
    }

    /// Overrides the detected language for one document without changing its bytes.
    ///
    /// # Errors
    /// Returns an error for an invalid/unknown language ID or unknown document.
    pub fn set_language(
        &self,
        document_id: &DocumentId,
        language_id: &str,
    ) -> Result<String, EditorError> {
        let language_id = LanguageId::new(language_id.to_owned())?;
        if !self
            .languages()
            .descriptors()
            .iter()
            .any(|descriptor| descriptor.id == language_id)
        {
            return Err(nulis_syntax::SyntaxError::UnknownLanguage(language_id).into());
        }
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| EditorError::StateUnavailable)?;
        let session = sessions
            .get_mut(document_id)
            .ok_or_else(|| EditorError::UnknownDocument(document_id.clone()))?;
        session.language_id = language_id;
        session.language_override = true;
        self.refresh_language_state(session);
        Ok(session.language_id.as_str().to_owned())
    }
}
