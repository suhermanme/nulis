//! Builds a runtime language, providers, and associations from approved
//! declarative plugin contributions.
//!
//! Everything here is data from a manifest that the plugin host already
//! validated. Building happens at install-adjacent state changes (approval,
//! enable, disable, uninstall), never while a document is being edited, and a
//! bad contribution only removes itself: its error is reported and the other
//! plugins keep working.

use nulis_language::{
    completion::{KeywordCompletion, OwnedSnippet, OwnedSnippetCompletion, WordCompletion},
    indent::RegexIndenter,
    CapabilityKind, Provider, ProviderId, ProviderInfo, ProviderOrigin,
};
use nulis_plugin_host::{
    PluginActivation, PluginCapability, PluginHighlightMode, PluginLanguageContribution,
    PluginPackage,
};
use nulis_syntax::{
    CommentSyntax, DeclarativeLexer, DeclarativeMode, DeclarativeString, LanguageDescriptor,
    LanguageId, LanguageModule, Vocabulary,
};
use std::hash::{Hash, Hasher};

/// One language ready to be registered, with the providers that belong to it.
#[derive(Debug)]
pub struct PluginLanguage {
    pub plugin_id: String,
    pub module: LanguageModule,
    /// True when the contribution only maps files to a language it does not
    /// define, so a built-in language may own it.
    pub association_only: bool,
    pub providers: Vec<(ProviderInfo, Provider)>,
}

#[derive(Debug, Default)]
pub struct PluginLanguageSync {
    pub languages: Vec<PluginLanguage>,
    pub errors: Vec<String>,
}

/// Builds every active plugin language. An inactive, unapproved, disabled, or
/// removed plugin contributes nothing.
pub fn build(
    packages: &[PluginPackage],
    activation: impl Fn(&PluginPackage) -> Option<PluginActivation>,
) -> PluginLanguageSync {
    let mut sync = PluginLanguageSync::default();
    for package in packages {
        let Some(activation) = activation(package) else {
            continue;
        };
        if !activation.enabled || !activation.capability_enabled(PluginCapability::Languages) {
            continue;
        }
        for language in &package.manifest.contributes.languages {
            match build_language(package, language, &activation) {
                Ok(built) => sync.languages.push(built),
                Err(error) => sync
                    .errors
                    .push(format!("{}: {error}", package.manifest.id)),
            }
        }
    }
    sync
}

fn build_language(
    package: &PluginPackage,
    language: &PluginLanguageContribution,
    activation: &PluginActivation,
) -> Result<PluginLanguage, String> {
    let id = LanguageId::new(language.id.clone()).map_err(|error| error.to_string())?;
    let descriptor = LanguageDescriptor {
        id: id.clone(),
        display_name: language.display_name.clone(),
        extensions: language.extensions.clone(),
        filenames: language.filenames.clone(),
    };
    let comments = CommentSyntax {
        line: language.comments.line.clone(),
        block: language.comments.block.clone(),
    };
    let highlight = language
        .highlight
        .as_ref()
        .filter(|_| activation.capability_enabled(PluginCapability::Highlight));
    let vocabulary = language
        .highlight
        .as_ref()
        .map(|highlight| Vocabulary {
            keywords: highlight.keywords.clone(),
            types: highlight.types.clone(),
            case_insensitive: highlight.case_insensitive,
            word_bytes: highlight
                .ident_chars
                .iter()
                .filter_map(|character| character.as_bytes().first().copied())
                .collect(),
        })
        .unwrap_or_default();
    let lexer = highlight.map(|highlight| DeclarativeLexer {
        mode: highlight.mode.map(mode_of),
        keywords: highlight.keywords.clone(),
        types: highlight.types.clone(),
        strings: highlight
            .strings
            .iter()
            .map(|string| DeclarativeString {
                open: string.open.clone(),
                close: string.close.clone(),
                escape: string.escape,
                multiline: string.multiline,
            })
            .collect(),
        case_insensitive: highlight.case_insensitive,
        ident_chars: highlight
            .ident_chars
            .iter()
            .filter_map(|character| character.as_bytes().first().copied())
            .collect(),
    });
    let module = LanguageModule::declarative(
        descriptor,
        lexer,
        comments,
        vocabulary.clone(),
        language.first_line.as_deref(),
    )
    .map_err(|error| error.to_string())?;

    let association_only = language.highlight.is_none()
        && language.indentation.is_none()
        && language.completion.is_none()
        && language.snippets.is_empty()
        && language.comments.line.is_none()
        && language.comments.block.is_none();

    let providers = build_providers(&package.manifest.id, language, activation, &id, &vocabulary)?;

    Ok(PluginLanguage {
        plugin_id: package.manifest.id.clone(),
        module,
        association_only,
        providers,
    })
}

fn build_providers(
    plugin_id: &str,
    language: &PluginLanguageContribution,
    activation: &PluginActivation,
    language_id: &LanguageId,
    vocabulary: &Vocabulary,
) -> Result<Vec<(ProviderInfo, Provider)>, String> {
    let mut providers = Vec::new();
    let origin = ProviderOrigin::Plugin(plugin_id.to_owned());
    let label = language.display_name.as_str();
    let mut add = |kind: CapabilityKind, suffix: &str, provider: Provider| {
        let Some(provider_id_value) = provider_id(plugin_id, language_id.as_str(), suffix) else {
            return;
        };
        providers.push((
            ProviderInfo {
                id: provider_id_value,
                origin: origin.clone(),
                kind,
                languages: vec![language_id.clone()],
                label: label.into(),
            },
            provider,
        ));
    };
    if language.highlight.is_some() && activation.capability_enabled(PluginCapability::Highlight) {
        add(CapabilityKind::Highlight, "highlight", Provider::Declared);
    }
    if let Some(indentation) = &language.indentation {
        if activation.capability_enabled(PluginCapability::Indentation) {
            let indenter = RegexIndenter::new(
                indentation.increase_after.as_deref(),
                indentation.outdent_on_type.as_deref(),
            )
            .map_err(|error| format!("{}: {error}", language.id))?;
            add(
                CapabilityKind::Indentation,
                "indent",
                Provider::Indent(std::sync::Arc::new(indenter)),
            );
        }
    }
    if let Some(completion) = &language.completion {
        if activation.capability_enabled(PluginCapability::Completion) {
            if completion.keywords {
                add(
                    CapabilityKind::Completion,
                    "completion-keywords",
                    Provider::Completion(std::sync::Arc::new(KeywordCompletion {
                        vocabulary: vocabulary.clone(),
                    })),
                );
            }
            if completion.words {
                add(
                    CapabilityKind::Completion,
                    "completion-words",
                    Provider::Completion(std::sync::Arc::new(WordCompletion)),
                );
            }
        }
    }
    if !language.snippets.is_empty() && activation.capability_enabled(PluginCapability::Snippets) {
        add(
            CapabilityKind::Completion,
            "snippets",
            Provider::Completion(std::sync::Arc::new(OwnedSnippetCompletion {
                snippets: language
                    .snippets
                    .iter()
                    .map(|snippet| OwnedSnippet {
                        prefix: snippet.prefix.clone(),
                        body: snippet.body.clone(),
                        description: snippet.description.clone(),
                    })
                    .collect(),
            })),
        );
    }
    drop(add);
    Ok(providers)
}

fn mode_of(mode: PluginHighlightMode) -> DeclarativeMode {
    match mode {
        PluginHighlightMode::Code => DeclarativeMode::Code,
        PluginHighlightMode::Markup => DeclarativeMode::Markup,
        PluginHighlightMode::Css => DeclarativeMode::Css,
        PluginHighlightMode::Delimited => DeclarativeMode::Delimited,
    }
}

/// Provider IDs are limited to 128 bytes and lowercase characters. Plugins
/// keep readable IDs; a long one is shortened with a stable hash suffix so
/// two long IDs cannot collide into one provider.
fn provider_id(plugin_id: &str, language: &str, kind: &str) -> Option<ProviderId> {
    let base = format!("plugin:{plugin_id}/{language}.{kind}").to_lowercase();
    if base.len() <= 128 {
        return ProviderId::new(base).ok();
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    base.hash(&mut hasher);
    let digest = format!("{:016x}", hasher.finish());
    let keep = 128_usize.saturating_sub(digest.len() + 1);
    ProviderId::new(format!("{}.{digest}", &base[..keep])).ok()
}

#[cfg(test)]
mod tests {
    use super::{provider_id, PluginLanguageSync};
    use nulis_language::{CapabilityKind, ProviderOrigin};
    use nulis_plugin_host::{PluginActivation, PluginCapability, PluginGrant, PluginPackage};
    use std::collections::BTreeSet;

    fn package(root: &std::path::Path) -> PluginPackage {
        std::fs::create_dir_all(root).expect("package root");
        std::fs::write(
            root.join("plugin.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
              "apiVersion": 2,
              "id": "acme.nim",
              "name": "Nim language",
              "version": "2.0.0",
              "engines": { "nulis": ">=0.1.0" },
              "contributes": { "languages": [{
                "id": "nim",
                "displayName": "Nim",
                "extensions": ["nim"],
                "firstLine": "^#!.*\\bnim\\b",
                "comments": { "line": "#" },
                "highlight": {
                  "keywords": ["proc", "let"],
                  "types": ["int"],
                  "strings": [{ "open": "\"", "close": "\"", "escape": true }],
                  "identChars": ["_"]
                },
                "indentation": { "increaseAfter": ":$" },
                "completion": { "keywords": true, "words": true },
                "snippets": [{ "prefix": "proc", "body": "proc $0", "description": "Procedure" }]
              }] }
            }))
            .expect("manifest"),
        )
        .expect("write manifest");
        PluginPackage::load(root).expect("package")
    }

    fn activation(disabled: &[PluginCapability], enabled: bool) -> PluginActivation {
        PluginActivation {
            grant: PluginGrant::default(),
            enabled,
            disabled_capabilities: disabled.iter().copied().collect::<BTreeSet<_>>(),
        }
    }

    #[test]
    fn an_unapproved_disabled_or_switched_off_plugin_contributes_nothing() {
        let root = tempfile::tempdir().expect("root");
        let package = package(root.path());
        assert!(super::build(std::slice::from_ref(&package), |_| None)
            .languages
            .is_empty());
        assert!(
            super::build(std::slice::from_ref(&package), |_| Some(activation(
                &[],
                false
            )))
            .languages
            .is_empty()
        );
        assert!(
            super::build(std::slice::from_ref(&package), |_| Some(activation(
                &[PluginCapability::Languages],
                true
            )))
            .languages
            .is_empty()
        );
    }

    #[test]
    fn an_active_plugin_builds_owned_language_data_and_providers() {
        let root = tempfile::tempdir().expect("root");
        let package = package(root.path());
        let sync = super::build(std::slice::from_ref(&package), |_| {
            Some(activation(&[], true))
        });
        assert!(sync.errors.is_empty(), "{:?}", sync.errors);
        assert_eq!(sync.languages.len(), 1);
        let language = &sync.languages[0];
        assert_eq!(language.plugin_id, "acme.nim");
        assert_eq!(language.module.descriptor().id.as_str(), "nim");
        assert!(!language.association_only);
        assert_eq!(language.module.descriptor().extensions, ["nim"]);
        let kinds = language
            .providers
            .iter()
            .map(|(info, _)| info.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                CapabilityKind::Highlight,
                CapabilityKind::Indentation,
                CapabilityKind::Completion,
                CapabilityKind::Completion,
                CapabilityKind::Completion,
            ]
        );
        assert!(language.providers.iter().all(|(info, _)| {
            info.origin == ProviderOrigin::Plugin("acme.nim".into())
                && info.id.as_str().starts_with("plugin:acme.nim/nim.")
        }));
        assert!(language
            .providers
            .iter()
            .any(|(info, _)| info.id.as_str().ends_with(".snippets")));
    }

    #[test]
    fn switching_off_one_family_keeps_the_rest_of_the_language() {
        let root = tempfile::tempdir().expect("root");
        let package = package(root.path());
        let sync = super::build(std::slice::from_ref(&package), |_| {
            Some(activation(&[PluginCapability::Highlight], true))
        });
        assert!(sync.errors.is_empty(), "{:?}", sync.errors);
        let language = &sync.languages[0];
        assert!(!language
            .providers
            .iter()
            .any(|(info, _)| info.kind == CapabilityKind::Highlight));
        assert!(language
            .providers
            .iter()
            .any(|(info, _)| info.kind == CapabilityKind::Completion));
        // The language itself remains registered and detectable.
        assert_eq!(language.module.descriptor().id.as_str(), "nim");
    }

    #[test]
    fn a_bad_contribution_is_reported_without_stopping_the_others() {
        let root = tempfile::tempdir().expect("root");
        let mut broken = package(root.path());
        broken.manifest.contributes.languages[0].first_line = Some("(".into());
        let good_root = root.path().join("good");
        let good = package(&good_root);
        let sync = super::build(&[broken, good], |_| Some(activation(&[], true)));
        assert_eq!(sync.languages.len(), 1);
        assert_eq!(sync.languages[0].plugin_id, "acme.nim");
        // The broken and the good packages share an ID here, so the report
        // names the ID and the reason; the good language still loads.
        assert_eq!(sync.errors.len(), 1);
        assert!(sync.errors[0].contains("acme.nim"));
        assert!(sync.errors[0].to_lowercase().contains("regex"));
    }

    #[test]
    fn provider_ids_stay_readable_and_bounded() {
        assert_eq!(
            provider_id("acme.nim", "nim", "highlight")
                .expect("id")
                .as_str(),
            "plugin:acme.nim/nim.highlight"
        );
        let long = "a".repeat(160);
        let first = provider_id(&long, "nim", "highlight").expect("id");
        let second = provider_id(&"b".repeat(160), "nim", "highlight").expect("id");
        assert!(first.as_str().len() <= 128);
        assert!(second.as_str().len() <= 128);
        assert_ne!(first, second);
        assert_eq!(
            provider_id(&long, "nim", "highlight").expect("stable"),
            first
        );
    }

    #[test]
    fn the_sync_value_is_usable_as_a_default() {
        let sync = PluginLanguageSync::default();
        assert_eq!(sync.languages.len(), 0);
        assert_eq!(sync.errors.len(), 0);
    }
}
