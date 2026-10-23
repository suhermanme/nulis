use super::{
    CapabilityError, CapabilityKind, CapabilityRegistry, DocumentText, FormatAssessment,
    FormatMode, FormatOutcome, Formatter, Provider, ProviderError, ProviderId, ProviderInfo,
    ProviderOrigin, TextProblem, Validator, MAX_CONSECUTIVE_FAILURES,
};
use nulis_syntax::LanguageId;
use std::sync::Arc;

fn language(id: &str) -> LanguageId {
    LanguageId::new(id).expect("valid language")
}

fn info(
    id: &str,
    origin: ProviderOrigin,
    kind: CapabilityKind,
    languages: &[&str],
) -> ProviderInfo {
    ProviderInfo {
        id: ProviderId::new(id).expect("valid provider"),
        origin,
        kind,
        languages: languages.iter().map(|item| language(item)).collect(),
        label: id.to_owned(),
    }
}

/// Reports a problem when the text contains "bad", and panics on "panic".
struct Checker;

impl Validator for Checker {
    fn validate(&self, input: &DocumentText<'_>) -> Result<Option<TextProblem>, ProviderError> {
        assert!(!input.text.contains("panic"), "checker panicked");
        Ok(input.text.find("bad").map(|offset| TextProblem {
            message: "bad word".into(),
            offset,
        }))
    }
}

struct Upper;

impl Formatter for Upper {
    fn modes(&self) -> &[FormatMode] {
        &[FormatMode::Beautify]
    }

    fn assess(&self, _input: &DocumentText<'_>) -> Result<FormatAssessment, ProviderError> {
        Ok(FormatAssessment::Ready)
    }

    fn format(
        &self,
        input: &DocumentText<'_>,
        _mode: FormatMode,
        _accept_loss: bool,
    ) -> Result<FormatOutcome, ProviderError> {
        let upper = input.text.to_uppercase();
        Ok(if upper == input.text {
            FormatOutcome::Unchanged
        } else {
            FormatOutcome::Replaced(upper)
        })
    }
}

fn plugin(id: &str) -> ProviderOrigin {
    ProviderOrigin::Plugin(id.into())
}

fn formatter() -> Provider {
    Provider::Formatter(Arc::new(Upper))
}

fn ids(registry: &CapabilityRegistry, lang: &str, kind: CapabilityKind) -> Vec<String> {
    registry
        .resolve_all(&language(lang), kind)
        .into_iter()
        .map(|resolved| resolved.info.id.as_str().to_owned())
        .collect()
}

#[test]
fn provider_ids_are_checked() {
    assert!(ProviderId::new("builtin.json.format").is_ok());
    assert!(ProviderId::new("plugin:acme.nim/indent").is_ok());
    for bad in ["", "Upper", "has space", &"x".repeat(129)] {
        assert!(
            matches!(
                ProviderId::new(bad),
                Err(CapabilityError::InvalidProviderId(_))
            ),
            "{bad}"
        );
    }
}

#[test]
fn registration_rejects_duplicates_mismatches_and_empty_language_lists() {
    let mut registry = CapabilityRegistry::new();
    registry
        .register(
            info(
                "builtin.json.format",
                ProviderOrigin::Builtin,
                CapabilityKind::Format,
                &["json"],
            ),
            formatter(),
        )
        .expect("register");
    assert!(matches!(
        registry.register(
            info(
                "builtin.json.format",
                ProviderOrigin::Builtin,
                CapabilityKind::Format,
                &["json"]
            ),
            formatter()
        ),
        Err(CapabilityError::DuplicateProvider(_))
    ));
    assert!(matches!(
        registry.register(
            info(
                "x.validate",
                ProviderOrigin::Builtin,
                CapabilityKind::Diagnostics,
                &["json"]
            ),
            formatter()
        ),
        Err(CapabilityError::WrongImplementation(_))
    ));
    assert!(matches!(
        registry.register(
            info(
                "x.none",
                ProviderOrigin::Builtin,
                CapabilityKind::Format,
                &[]
            ),
            formatter()
        ),
        Err(CapabilityError::NoLanguage(_))
    ));
    assert!(registry
        .register(
            info(
                "builtin.json.highlight",
                ProviderOrigin::Builtin,
                CapabilityKind::Highlight,
                &["json"]
            ),
            Provider::Declared
        )
        .is_ok());
}

#[test]
fn a_single_provider_kind_prefers_built_in_then_plugins_by_id_then_the_users_choice() {
    let mut registry = CapabilityRegistry::new();
    registry
        .register(
            info(
                "plugin.zeta",
                plugin("zeta"),
                CapabilityKind::Format,
                &["json"],
            ),
            formatter(),
        )
        .expect("register");
    registry
        .register(
            info(
                "plugin.alpha",
                plugin("alpha"),
                CapabilityKind::Format,
                &["json"],
            ),
            formatter(),
        )
        .expect("register");
    assert_eq!(
        ids(&registry, "json", CapabilityKind::Format),
        ["plugin.alpha"]
    );
    registry
        .register(
            info(
                "builtin.json.format",
                ProviderOrigin::Builtin,
                CapabilityKind::Format,
                &["json"],
            ),
            formatter(),
        )
        .expect("register");
    assert_eq!(
        ids(&registry, "json", CapabilityKind::Format),
        ["builtin.json.format"]
    );

    let json = language("json");
    registry.set_preference(
        &json,
        CapabilityKind::Format,
        Some(ProviderId::new("plugin.zeta").expect("id")),
    );
    assert_eq!(
        ids(&registry, "json", CapabilityKind::Format),
        ["plugin.zeta"]
    );
    // A choice of a provider that is off falls back to the default order.
    registry
        .set_provider_enabled(&ProviderId::new("plugin.zeta").expect("id"), false)
        .expect("known");
    assert_eq!(
        ids(&registry, "json", CapabilityKind::Format),
        ["builtin.json.format"]
    );
    registry.set_preference(&json, CapabilityKind::Format, None);
    assert_eq!(
        ids(&registry, "json", CapabilityKind::Format),
        ["builtin.json.format"]
    );
    // Other languages are unaffected.
    assert_eq!(
        ids(&registry, "yaml", CapabilityKind::Format),
        Vec::<String>::new()
    );
}

#[test]
fn merged_kinds_use_every_enabled_provider_in_order() {
    let mut registry = CapabilityRegistry::new();
    for (id, origin) in [
        (
            "lsp.json",
            ProviderOrigin::LanguageServer("vscode-json".into()),
        ),
        ("plugin.lint", plugin("lint")),
        ("builtin.json.validate", ProviderOrigin::Builtin),
    ] {
        registry
            .register(
                info(id, origin, CapabilityKind::Diagnostics, &["json"]),
                Provider::Validator(Arc::new(Checker)),
            )
            .expect("register");
    }
    assert_eq!(
        ids(&registry, "json", CapabilityKind::Diagnostics),
        ["builtin.json.validate", "plugin.lint", "lsp.json"]
    );
}

#[test]
fn switching_a_capability_off_for_a_language_hides_it_and_switching_on_restores_it() {
    let mut registry = CapabilityRegistry::new();
    registry
        .register(
            info(
                "builtin.format",
                ProviderOrigin::Builtin,
                CapabilityKind::Format,
                &["json", "xml"],
            ),
            formatter(),
        )
        .expect("register");
    let json = language("json");
    registry.set_capability_enabled(&json, CapabilityKind::Format, false);
    assert!(registry.resolve(&json, CapabilityKind::Format).is_none());
    assert!(registry.capabilities(&json).is_empty());
    assert!(registry
        .resolve(&language("xml"), CapabilityKind::Format)
        .is_some());
    registry.set_capability_enabled(&json, CapabilityKind::Format, true);
    registry.replace_disabled([(language("xml"), CapabilityKind::Format)]);
    assert!(registry
        .resolve(&language("xml"), CapabilityKind::Format)
        .is_none());
    registry.replace_disabled([]);
    assert!(registry
        .resolve(&language("xml"), CapabilityKind::Format)
        .is_some());
    assert_eq!(
        registry
            .capabilities(&json)
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [CapabilityKind::Format]
    );
}

#[test]
fn a_plugin_can_be_switched_off_and_removed_as_a_whole() {
    let mut registry = CapabilityRegistry::new();
    registry
        .register(
            info(
                "plugin.nim.format",
                plugin("acme.nim"),
                CapabilityKind::Format,
                &["nim"],
            ),
            formatter(),
        )
        .expect("register");
    registry
        .register(
            info(
                "plugin.nim.check",
                plugin("acme.nim"),
                CapabilityKind::Diagnostics,
                &["nim"],
            ),
            Provider::Validator(Arc::new(Checker)),
        )
        .expect("register");
    registry
        .register(
            info(
                "builtin.json.format",
                ProviderOrigin::Builtin,
                CapabilityKind::Format,
                &["json"],
            ),
            formatter(),
        )
        .expect("register");
    let nim = language("nim");
    registry.set_origin_enabled(&plugin("acme.nim"), false);
    assert!(registry.capabilities(&nim).is_empty());
    registry.set_origin_enabled(&plugin("acme.nim"), true);
    assert_eq!(registry.capabilities(&nim).len(), 2);
    assert_eq!(registry.unregister_origin(&plugin("acme.nim")), 2);
    assert!(registry.capabilities(&nim).is_empty());
    assert_eq!(registry.providers().len(), 1);
}

#[test]
fn a_panicking_provider_is_contained_and_set_aside_after_repeated_failures() {
    let mut registry = CapabilityRegistry::new();
    registry
        .register(
            info(
                "plugin.check",
                plugin("check"),
                CapabilityKind::Diagnostics,
                &["json"],
            ),
            Provider::Validator(Arc::new(Checker)),
        )
        .expect("register");
    let json = language("json");
    let run = |registry: &CapabilityRegistry, text: &str| {
        let resolved = registry
            .resolve_all(&json, CapabilityKind::Diagnostics)
            .pop()
            .expect("provider");
        let Provider::Validator(validator) = resolved.provider else {
            panic!("validator")
        };
        registry.call(&resolved.info, || {
            validator.validate(&DocumentText {
                language: &json,
                text,
            })
        })
    };

    assert_eq!(run(&registry, "fine"), Ok(None));
    assert_eq!(
        run(&registry, "a bad word"),
        Ok(Some(TextProblem {
            message: "bad word".into(),
            offset: 2
        }))
    );
    for attempt in 1..MAX_CONSECUTIVE_FAILURES {
        assert_eq!(
            run(&registry, "panic"),
            Err(ProviderError::Failed(
                "plugin.check stopped unexpectedly".into()
            )),
            "attempt {attempt}"
        );
    }
    // A success in between clears the count.
    assert_eq!(run(&registry, "fine"), Ok(None));
    for _ in 0..MAX_CONSECUTIVE_FAILURES {
        assert!(run(&registry, "panic").is_err());
    }
    assert!(registry
        .resolve_all(&json, CapabilityKind::Diagnostics)
        .is_empty());
    assert_eq!(
        registry.suspended(),
        [ProviderId::new("plugin.check").expect("id")]
    );
    // Switching it back on gives it another chance.
    registry
        .set_provider_enabled(&ProviderId::new("plugin.check").expect("id"), true)
        .expect("known");
    assert_eq!(run(&registry, "fine"), Ok(None));
}

#[test]
fn content_problems_and_refusals_do_not_count_as_failures() {
    let mut registry = CapabilityRegistry::new();
    let provider = info(
        "builtin.x",
        ProviderOrigin::Builtin,
        CapabilityKind::Format,
        &["json"],
    );
    registry
        .register(provider.clone(), formatter())
        .expect("register");
    for _ in 0..=MAX_CONSECUTIVE_FAILURES {
        let _ = registry.call::<()>(&provider, || {
            Err(ProviderError::Refused(
                "Minify is available for JSON only".into(),
            ))
        });
        let _ = registry.call::<()>(&provider, || {
            Err(ProviderError::Content(TextProblem {
                message: "invalid".into(),
                offset: 0,
            }))
        });
    }
    assert!(registry
        .resolve(&language("json"), CapabilityKind::Format)
        .is_some());
}
