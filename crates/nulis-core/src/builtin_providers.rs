//! The built-in language providers: Validate and Beautify for JSON, XML and
//! XSL, YAML, and property lists, Minify for JSON, and the Markdown preview.
//! The algorithms live in `format_tools`, `yaml_tools`, and `plist_tools`;
//! this module only presents them through the `nulis-language` contracts.

use crate::{editor::MAX_MARKDOWN_PREVIEW_BYTES, format_tools, plist_tools, yaml_tools};
use nulis_language::{
    CapabilityKind, CapabilityRegistry, DocumentText, FormatAssessment, FormatMode, FormatOutcome,
    Formatter, PreviewProvider, Provider, ProviderError, ProviderId, ProviderInfo, ProviderOrigin,
    TextProblem, Validator,
};
use nulis_syntax::LanguageId;
use std::sync::Arc;

fn content(error: format_tools::ToolError) -> ProviderError {
    ProviderError::Content(TextProblem {
        message: error.message,
        offset: error.offset,
    })
}

fn outcome(input: &DocumentText<'_>, formatted: String) -> FormatOutcome {
    if formatted == input.text {
        FormatOutcome::Unchanged
    } else {
        FormatOutcome::Replaced(formatted)
    }
}

/// Validates a document; `Ok(None)` means valid.
type Check = fn(&str) -> Result<(), format_tools::ToolError>;

struct CheckValidator(Check);

impl Validator for CheckValidator {
    fn validate(&self, input: &DocumentText<'_>) -> Result<Option<TextProblem>, ProviderError> {
        Ok((self.0)(input.text).err().map(|error| TextProblem {
            message: error.message,
            offset: error.offset,
        }))
    }
}

struct JsonFormatter;

impl Formatter for JsonFormatter {
    fn modes(&self) -> &[FormatMode] {
        &[FormatMode::Beautify, FormatMode::Compact]
    }

    fn assess(&self, _input: &DocumentText<'_>) -> Result<FormatAssessment, ProviderError> {
        Ok(FormatAssessment::Ready)
    }

    fn format(
        &self,
        input: &DocumentText<'_>,
        mode: FormatMode,
        _accept_loss: bool,
    ) -> Result<FormatOutcome, ProviderError> {
        format_tools::format_json(input.text, mode == FormatMode::Compact)
            .map(|formatted| outcome(input, formatted))
            .map_err(content)
    }
}

/// XML, XSL, and (after its own structure check) property lists: only the
/// whitespace between elements changes.
struct XmlFormatter {
    check: Option<Check>,
}

impl Formatter for XmlFormatter {
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
        self.check
            .map_or(Ok(()), |check| check(input.text))
            .and_then(|()| format_tools::format_xml(input.text))
            .map(|formatted| outcome(input, formatted))
            .map_err(content)
    }
}

struct YamlValidator;

impl Validator for YamlValidator {
    fn validate(&self, input: &DocumentText<'_>) -> Result<Option<TextProblem>, ProviderError> {
        Ok(yaml_tools::validate_yaml(input.text)
            .err()
            .map(|error| TextProblem {
                message: error.message,
                offset: error.offset,
            }))
    }
}

/// YAML Beautify rewrites the document from its values, so it may drop
/// comments and expand anchors (asks first), and cannot keep tags (refuses).
struct YamlFormatter;

impl YamlFormatter {
    fn note(features: yaml_tools::YamlFeatures) -> Option<String> {
        if features.tags {
            return Some(
                "Beautify cannot keep YAML tags such as !Ref, so it is not available for this document."
                    .into(),
            );
        }
        let mut losses = Vec::new();
        if features.comments {
            losses.push("removes comments");
        }
        if features.anchors {
            losses.push("writes anchors and aliases (&name, *name) out in full");
        }
        (!losses.is_empty()).then(|| {
            format!(
                "Beautify {}; values and key order are kept, but quoting and number spelling may change.",
                losses.join(" and ")
            )
        })
    }
}

impl Formatter for YamlFormatter {
    fn modes(&self) -> &[FormatMode] {
        &[FormatMode::Beautify]
    }

    fn may_lose_data(&self) -> bool {
        true
    }

    fn assess(&self, input: &DocumentText<'_>) -> Result<FormatAssessment, ProviderError> {
        let features = yaml_tools::validate_yaml(input.text).map_err(content)?;
        Ok(match Self::note(features) {
            Some(note) if features.tags => FormatAssessment::Unavailable(note),
            Some(note) => FormatAssessment::Lossy(note),
            None => FormatAssessment::Ready,
        })
    }

    fn format(
        &self,
        input: &DocumentText<'_>,
        _mode: FormatMode,
        accept_loss: bool,
    ) -> Result<FormatOutcome, ProviderError> {
        let features = yaml_tools::validate_yaml(input.text).map_err(content)?;
        if (features.comments || features.anchors) && !accept_loss && !features.tags {
            return Err(ProviderError::Refused(format!(
                "{} Beautify again and confirm to go ahead.",
                Self::note(features).unwrap_or_default()
            )));
        }
        yaml_tools::format_yaml(input.text)
            .map(|formatted| outcome(input, formatted))
            .map_err(content)
    }
}

struct MarkdownPreview;

impl PreviewProvider for MarkdownPreview {
    fn kind(&self) -> &'static str {
        "markdown"
    }

    fn max_source_bytes(&self) -> u64 {
        MAX_MARKDOWN_PREVIEW_BYTES
    }
}

fn info(id: &str, kind: CapabilityKind, languages: &[&str], label: &str) -> ProviderInfo {
    ProviderInfo {
        id: ProviderId::new(id).expect("built-in provider ID is valid"),
        origin: ProviderOrigin::Builtin,
        kind,
        languages: languages
            .iter()
            .map(|language| LanguageId::new(*language).expect("built-in language ID is valid"))
            .collect(),
        label: label.into(),
    }
}

/// A registry holding every built-in provider, in the order their labels
/// are listed to the user ("JSON, XML, YAML, and plist").
#[must_use]
#[allow(clippy::too_many_lines)] // One table listing every built-in provider.
pub fn builtin_registry() -> CapabilityRegistry {
    let validate_plist: Check = plist_tools::validate_plist;
    let providers: Vec<(ProviderInfo, Provider)> = vec![
        (
            info(
                "builtin.json.validate",
                CapabilityKind::Diagnostics,
                &["json"],
                "JSON",
            ),
            Provider::Validator(Arc::new(CheckValidator(format_tools::validate_json))),
        ),
        (
            info(
                "builtin.json.format",
                CapabilityKind::Format,
                &["json"],
                "JSON",
            ),
            Provider::Formatter(Arc::new(JsonFormatter)),
        ),
        (
            info(
                "builtin.json.compact",
                CapabilityKind::Compact,
                &["json"],
                "JSON",
            ),
            Provider::Formatter(Arc::new(JsonFormatter)),
        ),
        (
            info(
                "builtin.xml.validate",
                CapabilityKind::Diagnostics,
                &["xml", "xsl"],
                "XML",
            ),
            Provider::Validator(Arc::new(CheckValidator(format_tools::validate_xml))),
        ),
        (
            info(
                "builtin.xml.format",
                CapabilityKind::Format,
                &["xml", "xsl"],
                "XML",
            ),
            Provider::Formatter(Arc::new(XmlFormatter { check: None })),
        ),
        (
            info(
                "builtin.yaml.validate",
                CapabilityKind::Diagnostics,
                &["yaml"],
                "YAML",
            ),
            Provider::Validator(Arc::new(YamlValidator)),
        ),
        (
            info(
                "builtin.yaml.format",
                CapabilityKind::Format,
                &["yaml"],
                "YAML",
            ),
            Provider::Formatter(Arc::new(YamlFormatter)),
        ),
        (
            info(
                "builtin.plist.validate",
                CapabilityKind::Diagnostics,
                &["plist"],
                "plist",
            ),
            Provider::Validator(Arc::new(CheckValidator(validate_plist))),
        ),
        (
            info(
                "builtin.plist.format",
                CapabilityKind::Format,
                &["plist"],
                "plist",
            ),
            Provider::Formatter(Arc::new(XmlFormatter {
                check: Some(validate_plist),
            })),
        ),
        (
            info(
                "builtin.markdown.preview",
                CapabilityKind::Preview,
                &["markdown"],
                "Markdown",
            ),
            Provider::Preview(Arc::new(MarkdownPreview)),
        ),
    ];
    let mut registry = CapabilityRegistry::new();
    for (info, provider) in providers {
        registry
            .register(info, provider)
            .expect("built-in providers are unique and well formed");
    }
    registry
}
