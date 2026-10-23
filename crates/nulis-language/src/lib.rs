//! Language capabilities: what a language can do, which provider does it,
//! and calling providers so that one failing provider cannot take the
//! editor down.
//!
//! The crate holds contracts and bookkeeping only. Providers never change a
//! document: they return text or problems, and the core applies results with
//! its usual revision checks and undo records. See
//! `docs/LANGUAGE_CAPABILITIES.md`.

pub mod indent;

use nulis_syntax::LanguageId;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex},
};
use thiserror::Error;

/// Consecutive failures after which a provider is set aside for the session.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 3;
const MAX_PROVIDER_ID_BYTES: usize = 128;

/// One kind of feature a language can have.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityKind {
    Highlight,
    Comment,
    Indentation,
    Completion,
    Diagnostics,
    Format,
    Compact,
    Preview,
    Commands,
    Symbols,
}

impl CapabilityKind {
    /// Kinds whose providers all contribute (their results are merged);
    /// every other kind uses exactly one provider.
    #[must_use]
    pub const fn merges(self) -> bool {
        matches!(self, Self::Completion | Self::Diagnostics | Self::Commands)
    }
}

/// A stable provider name such as `builtin.json.format`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    /// # Errors
    /// Returns an error when the ID is empty, too long, or has characters
    /// other than lowercase ASCII letters, digits, and `.-_:/`.
    pub fn new(value: impl Into<String>) -> Result<Self, CapabilityError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_PROVIDER_ID_BYTES
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_' | b':' | b'/')
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(CapabilityError::InvalidProviderId(value))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where a provider comes from. For single-provider kinds, earlier variants
/// win when the user has not chosen.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProviderOrigin {
    Builtin,
    /// A plugin, by plugin ID.
    Plugin(String),
    /// A language server, by server ID.
    LanguageServer(String),
}

/// What a provider offers, for menus, Settings, and resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderInfo {
    pub id: ProviderId,
    pub origin: ProviderOrigin,
    pub kind: CapabilityKind,
    pub languages: Vec<LanguageId>,
    /// Shown to the user, as in "Validate JSON".
    pub label: String,
}

/// The whole text of a document, decoded and size-checked by the core.
#[derive(Clone, Copy, Debug)]
pub struct DocumentText<'a> {
    pub language: &'a LanguageId,
    pub text: &'a str,
}

/// Something wrong in the document, at a byte offset into the given text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextProblem {
    pub message: String,
    pub offset: usize,
}

/// Why a provider call did not produce a result.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProviderError {
    /// The document has a problem the user can fix.
    #[error("{}", .0.message)]
    Content(TextProblem),
    /// The provider declines this document or request, with a reason for the user.
    #[error("{0}")]
    Refused(String),
    /// The provider itself failed. Counts toward setting it aside.
    #[error("{0}")]
    Failed(String),
}

/// Checks a document.
pub trait Validator: Send + Sync {
    /// `Ok(None)` means valid.
    ///
    /// # Errors
    /// Returns `Refused` or `Failed` when the check could not run.
    fn validate(&self, input: &DocumentText<'_>) -> Result<Option<TextProblem>, ProviderError>;
}

/// The two ways of rewriting a document's layout.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FormatMode {
    /// Indent for reading ("Beautify").
    Beautify,
    /// Remove insignificant whitespace ("Minify").
    Compact,
}

/// Whether formatting can run on a document as it is.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatAssessment {
    Ready,
    /// It would lose something; the sentence says what. Runs only with consent.
    Lossy(String),
    /// It cannot run on this document; the sentence says why.
    Unavailable(String),
}

/// The result of formatting: nothing to change, or the new text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatOutcome {
    Unchanged,
    Replaced(String),
}

/// Rewrites a document's layout without changing its meaning.
pub trait Formatter: Send + Sync {
    /// The modes this formatter supports.
    fn modes(&self) -> &[FormatMode];

    /// Whether formatting can drop something (comments, anchors), so the
    /// editor asks `assess` first and gets the user's consent.
    fn may_lose_data(&self) -> bool {
        false
    }

    /// Says, before formatting, whether the result would lose anything.
    ///
    /// # Errors
    /// Returns `Content` when the document is invalid.
    fn assess(&self, input: &DocumentText<'_>) -> Result<FormatAssessment, ProviderError>;

    /// # Errors
    /// Returns `Content` for an invalid document and `Refused` for an
    /// unsupported mode or an unaccepted loss.
    fn format(
        &self,
        input: &DocumentText<'_>,
        mode: FormatMode,
        accept_loss: bool,
    ) -> Result<FormatOutcome, ProviderError>;
}

/// A rendered view of a document. The core serves bounded source; the
/// interface picks a renderer by `kind`.
pub trait PreviewProvider: Send + Sync {
    fn kind(&self) -> &str;
    fn max_source_bytes(&self) -> u64;
}

/// The implementation behind a registered provider.
#[derive(Clone)]
pub enum Provider {
    Validator(Arc<dyn Validator>),
    Formatter(Arc<dyn Formatter>),
    Preview(Arc<dyn PreviewProvider>),
    /// Served by another part of the editor (highlighting and comments come
    /// from `nulis-syntax`); registered so it can be listed and switched off.
    Declared,
}

impl std::fmt::Debug for Provider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Validator(_) => "Validator",
            Self::Formatter(_) => "Formatter",
            Self::Preview(_) => "Preview",
            Self::Declared => "Declared",
        })
    }
}

impl Provider {
    const fn fits(&self, kind: CapabilityKind) -> bool {
        match self {
            Self::Validator(_) => matches!(kind, CapabilityKind::Diagnostics),
            Self::Formatter(_) => matches!(kind, CapabilityKind::Format | CapabilityKind::Compact),
            Self::Preview(_) => matches!(kind, CapabilityKind::Preview),
            Self::Declared => true,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CapabilityError {
    #[error("invalid provider ID {0:?}")]
    InvalidProviderId(String),
    #[error("provider {0:?} is already registered")]
    DuplicateProvider(ProviderId),
    #[error("unknown provider {0:?}")]
    UnknownProvider(ProviderId),
    #[error("provider {0:?} does not implement the capability it declares")]
    WrongImplementation(ProviderId),
    #[error("provider {0:?} declares no language")]
    NoLanguage(ProviderId),
}

#[derive(Debug)]
struct Entry {
    info: ProviderInfo,
    provider: Provider,
    enabled: bool,
}

/// A provider chosen for a language and capability.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub info: ProviderInfo,
    pub provider: Provider,
}

/// Every registered provider, with the user's switches and choices.
#[derive(Debug, Default)]
pub struct CapabilityRegistry {
    entries: Vec<Entry>,
    disabled: BTreeSet<(LanguageId, CapabilityKind)>,
    preferences: BTreeMap<(LanguageId, CapabilityKind), ProviderId>,
    failures: Mutex<BTreeMap<ProviderId, u32>>,
}

impl CapabilityRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// # Errors
    /// Returns an error for a duplicate ID, an implementation that does not
    /// match the declared kind, or a provider without languages.
    pub fn register(
        &mut self,
        info: ProviderInfo,
        provider: Provider,
    ) -> Result<(), CapabilityError> {
        if self.entries.iter().any(|entry| entry.info.id == info.id) {
            return Err(CapabilityError::DuplicateProvider(info.id));
        }
        if !provider.fits(info.kind) {
            return Err(CapabilityError::WrongImplementation(info.id));
        }
        if info.languages.is_empty() {
            return Err(CapabilityError::NoLanguage(info.id));
        }
        self.entries.push(Entry {
            info,
            provider,
            enabled: true,
        });
        Ok(())
    }

    /// Removes every provider from one origin (a removed plugin, a stopped
    /// language server) and returns how many there were.
    pub fn unregister_origin(&mut self, origin: &ProviderOrigin) -> usize {
        let before = self.entries.len();
        self.entries.retain(|entry| &entry.info.origin != origin);
        before - self.entries.len()
    }

    /// Switches one provider on or off.
    ///
    /// # Errors
    /// Returns an error for an unknown provider.
    pub fn set_provider_enabled(
        &mut self,
        id: &ProviderId,
        enabled: bool,
    ) -> Result<(), CapabilityError> {
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| &entry.info.id == id)
            .ok_or_else(|| CapabilityError::UnknownProvider(id.clone()))?;
        entry.enabled = enabled;
        if enabled {
            self.reset_failures(id);
        }
        Ok(())
    }

    /// Switches every provider from one origin (a plugin) on or off.
    pub fn set_origin_enabled(&mut self, origin: &ProviderOrigin, enabled: bool) {
        for entry in self
            .entries
            .iter_mut()
            .filter(|entry| &entry.info.origin == origin)
        {
            entry.enabled = enabled;
        }
    }

    /// Switches a capability off (or back on) for one language, whatever provides it.
    pub fn set_capability_enabled(
        &mut self,
        language: &LanguageId,
        kind: CapabilityKind,
        enabled: bool,
    ) {
        let key = (language.clone(), kind);
        if enabled {
            self.disabled.remove(&key);
        } else {
            self.disabled.insert(key);
        }
    }

    /// Replaces every per-language switch at once: the listed capabilities
    /// are off, every other one is on.
    pub fn replace_disabled(
        &mut self,
        disabled: impl IntoIterator<Item = (LanguageId, CapabilityKind)>,
    ) {
        self.disabled = disabled.into_iter().collect();
    }

    #[must_use]
    pub fn is_capability_enabled(&self, language: &LanguageId, kind: CapabilityKind) -> bool {
        !self.disabled.contains(&(language.clone(), kind))
    }

    /// Chooses the provider for a single-provider capability; `None` returns
    /// to the default order. A choice of a provider that is missing or off is
    /// ignored.
    pub fn set_preference(
        &mut self,
        language: &LanguageId,
        kind: CapabilityKind,
        provider: Option<ProviderId>,
    ) {
        let key = (language.clone(), kind);
        match provider {
            Some(provider) => {
                self.preferences.insert(key, provider);
            }
            None => {
                self.preferences.remove(&key);
            }
        }
    }

    /// The providers that serve a capability for a language, in the order
    /// they are used: for a single-provider kind, at most one.
    #[must_use]
    pub fn resolve_all(&self, language: &LanguageId, kind: CapabilityKind) -> Vec<Resolved> {
        if !self.is_capability_enabled(language, kind) {
            return Vec::new();
        }
        let failures = self
            .failures
            .lock()
            .map(|failures| failures.clone())
            .unwrap_or_default();
        let mut candidates: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.enabled
                    && entry.info.kind == kind
                    && entry.info.languages.contains(language)
                    && failures.get(&entry.info.id).copied().unwrap_or(0) < MAX_CONSECUTIVE_FAILURES
            })
            .collect();
        let preferred = self.preferences.get(&(language.clone(), kind));
        candidates.sort_by(|left, right| {
            // The user's choice first, then by origin, then by ID.
            let not_chosen = |entry: &Entry| Some(&entry.info.id) != preferred;
            (not_chosen(left), &left.info.origin, &left.info.id).cmp(&(
                not_chosen(right),
                &right.info.origin,
                &right.info.id,
            ))
        });
        if !kind.merges() {
            candidates.truncate(1);
        }
        candidates
            .into_iter()
            .map(|entry| Resolved {
                info: entry.info.clone(),
                provider: entry.provider.clone(),
            })
            .collect()
    }

    /// The provider that serves a single-provider capability, if any.
    #[must_use]
    pub fn resolve(&self, language: &LanguageId, kind: CapabilityKind) -> Option<Resolved> {
        self.resolve_all(language, kind).into_iter().next()
    }

    /// Every capability a language has right now, with the chosen provider's label.
    #[must_use]
    pub fn capabilities(&self, language: &LanguageId) -> BTreeMap<CapabilityKind, ProviderInfo> {
        let kinds: BTreeSet<CapabilityKind> = self
            .entries
            .iter()
            .filter(|entry| entry.info.languages.contains(language))
            .map(|entry| entry.info.kind)
            .collect();
        kinds
            .into_iter()
            .filter_map(|kind| {
                self.resolve(language, kind)
                    .map(|resolved| (kind, resolved.info))
            })
            .collect()
    }

    /// Every registered provider with its on/off state, for Settings.
    #[must_use]
    pub fn providers(&self) -> Vec<(ProviderInfo, bool)> {
        self.entries
            .iter()
            .map(|entry| (entry.info.clone(), entry.enabled))
            .collect()
    }

    /// Providers set aside after repeated failures.
    #[must_use]
    pub fn suspended(&self) -> Vec<ProviderId> {
        self.failures
            .lock()
            .map(|failures| {
                failures
                    .iter()
                    .filter(|(_, count)| **count >= MAX_CONSECUTIVE_FAILURES)
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Runs one provider call. A panic becomes `ProviderError::Failed`;
    /// `Failed` results count toward setting the provider aside, and a
    /// result of any other kind clears the count.
    ///
    /// # Errors
    /// Returns the provider's error, or `Failed` when it panicked.
    pub fn call<T>(
        &self,
        provider: &ProviderInfo,
        run: impl FnOnce() -> Result<T, ProviderError>,
    ) -> Result<T, ProviderError> {
        let result = catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|_| {
            Err(ProviderError::Failed(format!(
                "{} stopped unexpectedly",
                provider.label
            )))
        });
        match &result {
            Err(ProviderError::Failed(_)) => {
                if let Ok(mut failures) = self.failures.lock() {
                    *failures.entry(provider.id.clone()).or_insert(0) += 1;
                }
            }
            _ => self.reset_failures(&provider.id),
        }
        result
    }

    fn reset_failures(&self, id: &ProviderId) {
        if let Ok(mut failures) = self.failures.lock() {
            failures.remove(id);
        }
    }
}

#[cfg(test)]
mod tests;
