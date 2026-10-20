//! Validated, executable-code-free theme data.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;

mod contrast;
pub use contrast::{
    Rgb, MIN_DISABLED_ICON_CONTRAST, MIN_ICON_CONTRAST, MIN_MUTED_TEXT_CONTRAST,
    MIN_SELECTED_TEXT_CONTRAST, MIN_SELECTION_CONTRAST, MIN_SELECTION_LINE_CONTRAST,
};

pub const THEME_SCHEMA_VERSION: u32 = 1;
pub const THEME_SCHEMA_URI: &str = "https://schemas.example.invalid/nulis/theme/v1.json";
pub const MAX_THEME_BYTES: u64 = 256 * 1024;
pub const MAX_USER_THEMES: usize = 64;
pub const THEME_TOKENS: &[&str] = &[
    "editor.background",
    "editor.foreground",
    "editor.selection",
    "editor.currentLine",
    "editor.lineNumber",
    "editor.caret",
    "editor.gutter",
    "editor.focus",
    "syntax.comment",
    "syntax.keyword",
    "syntax.string",
    "syntax.number",
    "syntax.function",
    "syntax.type",
    "syntax.property",
    "syntax.escape",
    "syntax.invalid",
    "ui.background",
    "ui.panel",
    "ui.surface",
    "ui.raised",
    "ui.border",
    "ui.text",
    "ui.mutedText",
    "ui.accent",
    "ui.accentForeground",
    "ui.focus",
    "ui.danger",
    "ui.warning",
    "ui.success",
    "ui.menu",
    "ui.toolbar",
    "ui.tab",
    "ui.statusBar",
    "ui.statusBarText",
    "ui.inputBackground",
    "ui.inputBorder",
    "ui.hover",
    "ui.overlay",
    "terminal.background",
    "terminal.foreground",
    "terminal.cursor",
    "diff.inserted",
    "diff.deleted",
    "diff.modified",
];
/// Tokens a theme may leave out; Nulis derives them from its other colors.
pub const OPTIONAL_THEME_TOKENS: &[&str] = &["ui.icon", "ui.iconDisabled"];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    Dark,
    Light,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub id: String,
    pub name: String,
    pub version: u32,
    pub kind: ThemeKind,
    pub colors: BTreeMap<String, String>,
}

impl Theme {
    /// Validates IDs and all present color values.
    ///
    /// # Errors
    /// Returns a typed error for an invalid ID, missing token, or non-hex color.
    pub fn validate(&self) -> Result<(), ThemeError> {
        validate_id(&self.id)?;
        if self.version == 0 || self.name.trim().is_empty() || self.name.len() > 128 {
            return Err(ThemeError::InvalidMetadata);
        }
        for &required in THEME_TOKENS {
            if !self.colors.contains_key(required) {
                return Err(ThemeError::MissingToken((*required).into()));
            }
        }
        for (token, color) in &self.colors {
            if !THEME_TOKENS.contains(&token.as_str())
                && !OPTIONAL_THEME_TOKENS.contains(&token.as_str())
            {
                return Err(ThemeError::UnknownToken(token.clone()));
            }
            let valid_length = color.len() == 7 || color.len() == 9;
            if !valid_length
                || !color.starts_with('#')
                || !color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(ThemeError::InvalidColor {
                    token: token.clone(),
                    color: color.clone(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeRecord {
    #[serde(flatten)]
    pub theme: Theme,
    pub built_in: bool,
}

#[derive(Clone, Debug)]
pub struct ThemeCatalog {
    directory: PathBuf,
}

impl ThemeCatalog {
    #[must_use]
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    /// Lists validated built-in and user themes. Invalid user files are ignored.
    ///
    /// # Errors
    /// Returns an I/O error if the user-theme directory cannot be inspected.
    pub fn list(&self) -> Result<Vec<ThemeRecord>, ThemeError> {
        let mut themes = built_in_themes()?;
        let mut ids = themes
            .iter()
            .map(|record| record.theme.id.clone())
            .collect::<BTreeSet<_>>();
        if self.directory.exists() {
            let metadata = fs::symlink_metadata(&self.directory)?;
            if !metadata.file_type().is_dir() {
                return Err(ThemeError::InvalidFile);
            }
            let mut entries = fs::read_dir(&self.directory)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(fs::DirEntry::file_name);
            for entry in entries
                .into_iter()
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .ends_with(".nulis-theme.json")
                })
                .take(MAX_USER_THEMES)
            {
                let path = entry.path();
                let Ok(mut theme) = read_theme(&path) else {
                    continue;
                };
                theme.ensure_readable();
                if ids.insert(theme.id.clone()) {
                    themes.push(ThemeRecord {
                        theme,
                        built_in: false,
                    });
                }
            }
        }
        // By name: "Nulis · …" comes before "NvChad · …", so Nulis's own themes lead.
        themes.sort_by(|left, right| left.theme.name.cmp(&right.theme.name));
        Ok(themes)
    }

    /// Installs a validated Nulis JSON theme or a declarative `NvChad` Lua palette.
    ///
    /// # Errors
    /// Returns an error for invalid files, built-in shadowing, duplicates, or I/O failures.
    pub fn install(&self, source: &Path) -> Result<ThemeRecord, ThemeError> {
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(ThemeError::InvalidFile)?;
        let theme = if file_name.ends_with(".nulis-theme.json") {
            read_theme(source)?
        } else if source
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lua"))
        {
            read_nvchad_theme(source)?
        } else {
            return Err(ThemeError::UnsupportedFormat);
        };
        self.install_theme(theme)
    }

    fn install_theme(&self, theme: Theme) -> Result<ThemeRecord, ThemeError> {
        if built_in_themes()?
            .iter()
            .any(|record| record.theme.id == theme.id)
        {
            return Err(ThemeError::BuiltIn(theme.id));
        }
        let current = self.list()?;
        if current.iter().any(|record| record.theme.id == theme.id) {
            return Err(ThemeError::Duplicate(theme.id));
        }
        if current.iter().filter(|record| !record.built_in).count() >= MAX_USER_THEMES {
            return Err(ThemeError::TooManyThemes);
        }
        fs::create_dir_all(&self.directory)?;
        let target = self
            .directory
            .join(format!("{}.nulis-theme.json", theme.id));
        let bytes = serde_json::to_vec_pretty(&theme)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist_noclobber(&target)
            .map_err(|error| error.error)?;
        // The file keeps the author's colors; what Nulis shows meets the contrast floors.
        let mut theme = theme;
        theme.ensure_readable();
        Ok(ThemeRecord {
            theme,
            built_in: false,
        })
    }

    /// Removes a user theme by validated ID.
    ///
    /// # Errors
    /// Returns an error for built-ins, missing themes, unsafe files, or I/O failures.
    pub fn uninstall(&self, id: &str) -> Result<(), ThemeError> {
        validate_id(id)?;
        if built_in_themes()?
            .iter()
            .any(|record| record.theme.id == id)
        {
            return Err(ThemeError::BuiltIn(id.into()));
        }
        let path = self.directory.join(format!("{id}.nulis-theme.json"));
        if !path.exists() {
            return Err(ThemeError::NotFound(id.into()));
        }
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err(ThemeError::InvalidFile);
        }
        fs::remove_file(path)?;
        Ok(())
    }
}

fn built_in_themes() -> Result<Vec<ThemeRecord>, ThemeError> {
    let mut themes = [
        include_str!("../../../themes/nulis-ember-dark.nulis-theme.json"),
        include_str!("../../../themes/nulis-paper-light.nulis-theme.json"),
    ]
    .into_iter()
    .map(serde_json::from_str::<Theme>)
    .collect::<Result<Vec<_>, _>>()?;
    themes.extend(serde_json::from_str::<Vec<Theme>>(include_str!(
        "../../../themes/nvchad-catalog.json"
    ))?);
    themes
        .into_iter()
        .map(|mut theme| {
            theme.validate()?;
            theme.ensure_readable();
            Ok(ThemeRecord {
                theme,
                built_in: true,
            })
        })
        .collect()
}

/// Reads and validates a theme file as written; see [`Theme::ensure_readable`].
fn read_theme(path: &Path) -> Result<Theme, ThemeError> {
    let theme: Theme = serde_json::from_slice(&read_bounded_file(path)?)?;
    theme.validate()?;
    Ok(theme)
}

fn read_nvchad_theme(path: &Path) -> Result<Theme, ThemeError> {
    let bytes = read_bounded_file(path)?;
    let source = std::str::from_utf8(&bytes).map_err(|_| ThemeError::InvalidUtf8)?;
    let source_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or(ThemeError::InvalidFile)?;
    import_nvchad_theme(source, source_name)
}

fn read_bounded_file(path: &Path) -> Result<Vec<u8>, ThemeError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_THEME_BYTES {
        return Err(ThemeError::InvalidFile);
    }
    let bytes = fs::read(path)?;
    if bytes.len() as u64 > MAX_THEME_BYTES {
        return Err(ThemeError::InvalidFile);
    }
    Ok(bytes)
}

/// Converts a declarative NvChad/Base46 palette into Nulis semantic colors.
///
/// The importer extracts literal palette tables and never evaluates Lua.
///
/// # Errors
/// Returns an error when required Base46 fields are absent, dynamic, or invalid.
///
/// # Panics
/// Panics only if an internal mapping references a `base_16` key omitted from the required-key check.
#[allow(clippy::too_many_lines)] // The semantic token mapping is intentionally explicit and auditable.
pub fn import_nvchad_theme(source: &str, source_name: &str) -> Result<Theme, ThemeError> {
    let source = strip_lua_comments(source)?;
    let local_colors = parse_local_colors(&source);
    let base_30 = parse_lua_color_table(
        &extract_lua_table(&source, "M.base_30")?,
        None,
        &local_colors,
    )?;
    let base_16 = parse_lua_color_table(
        &extract_lua_table(&source, "M.base_16")?,
        Some(&base_30),
        &local_colors,
    )?;
    for key in [
        "base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07", "base08",
        "base09", "base0A", "base0B", "base0C", "base0D", "base0E", "base0F",
    ] {
        if !base_16.contains_key(key) {
            return Err(ThemeError::NvChad(format!("M.base_16 is missing {key}")));
        }
    }
    let kind = match extract_lua_string_assignment(&source, "M.type")?.as_str() {
        "dark" => ThemeKind::Dark,
        "light" => ThemeKind::Light,
        value => {
            return Err(ThemeError::NvChad(format!(
                "M.type must be dark or light, not {value}"
            )))
        }
    };
    let id_suffix = slugify(source_name);
    if id_suffix.is_empty() {
        return Err(ThemeError::NvChad(
            "the source filename has no usable name".into(),
        ));
    }

    let base = |key: &str| {
        base_16
            .get(key)
            .expect("required Base46 color was checked")
            .as_str()
    };
    let ui = |key: &str, fallback: &str| {
        base_30
            .get(key)
            .map_or_else(|| base(fallback), String::as_str)
    };
    let mut colors = BTreeMap::new();
    let mut set = |token: &str, color: &str| {
        colors.insert(token.to_owned(), color.to_owned());
    };

    set("editor.background", base("base00"));
    set("editor.foreground", base("base05"));
    set("editor.selection", base("base02"));
    set("editor.currentLine", base("base01"));
    set("editor.lineNumber", base("base03"));
    set("editor.caret", ui("blue", "base0D"));
    set("editor.gutter", ui("black", "base00"));
    set("editor.focus", ui("nord_blue", "base0D"));
    set("syntax.comment", base("base03"));
    set("syntax.keyword", base("base0E"));
    set("syntax.string", base("base0B"));
    set("syntax.number", base("base09"));
    set("syntax.function", base("base0D"));
    set("syntax.type", base("base0A"));
    set("syntax.property", base("base08"));
    set("syntax.escape", base("base0C"));
    set("syntax.invalid", ui("red", "base08"));
    set("ui.background", ui("darker_black", "base00"));
    set("ui.panel", ui("black", "base00"));
    set("ui.surface", ui("black2", "base01"));
    set("ui.raised", ui("one_bg", "base02"));
    set("ui.border", ui("line", "base02"));
    set("ui.text", ui("white", "base05"));
    // `grey_fg` is meant for faint text such as comments; secondary text and
    // icons start from the lighter grey and are strengthened below if needed.
    set("ui.mutedText", ui("light_grey", "base04"));
    set("ui.accent", ui("blue", "base0D"));
    set("ui.accentForeground", base("base00"));
    set("ui.focus", ui("nord_blue", "base0D"));
    set("ui.danger", ui("red", "base08"));
    set("ui.warning", ui("yellow", "base0A"));
    set("ui.success", ui("green", "base0B"));
    set("ui.menu", ui("black2", "base01"));
    set("ui.toolbar", ui("darker_black", "base00"));
    set("ui.tab", ui("one_bg", "base02"));
    set("ui.statusBar", ui("statusline_bg", "base01"));
    set("ui.statusBarText", base("base05"));
    set("ui.inputBackground", ui("one_bg", "base01"));
    set("ui.inputBorder", ui("line", "base02"));
    set("ui.hover", ui("one_bg2", "base02"));
    let overlay = with_alpha(
        if kind == ThemeKind::Dark {
            base("base00")
        } else {
            base("base05")
        },
        if kind == ThemeKind::Dark { "CC" } else { "55" },
    );
    set("ui.overlay", &overlay);
    set("terminal.background", base("base00"));
    set("terminal.foreground", base("base05"));
    set("terminal.cursor", ui("blue", "base0D"));
    set(
        "diff.inserted",
        &mix_hex(base("base00"), ui("green", "base0B"), 22)?,
    );
    set(
        "diff.deleted",
        &mix_hex(base("base00"), ui("red", "base08"), 22)?,
    );
    set(
        "diff.modified",
        &mix_hex(base("base00"), ui("yellow", "base0A"), 22)?,
    );

    let mut theme = Theme {
        schema: Some(THEME_SCHEMA_URI.into()),
        id: format!("nvchad.{id_suffix}"),
        name: format!("NvChad · {}", display_name(source_name)),
        version: THEME_SCHEMA_VERSION,
        kind,
        colors,
    };
    theme.validate()?;
    theme.ensure_readable();
    Ok(theme)
}

fn strip_lua_comments(source: &str) -> Result<String, ThemeError> {
    let bytes = source.as_bytes();
    let mut result = bytes.to_vec();
    let mut index = 0;
    let mut quote = None;
    while index < bytes.len() {
        if let Some(delimiter) = quote {
            if bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
            } else {
                if bytes[index] == delimiter {
                    quote = None;
                }
                index += 1;
            }
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"') {
            quote = Some(bytes[index]);
            index += 1;
            continue;
        }
        if bytes[index..].starts_with(b"--[[") {
            let start = index;
            index += 4;
            while index + 1 < bytes.len() && !bytes[index..].starts_with(b"]]") {
                index += 1;
            }
            if index + 1 >= bytes.len() {
                return Err(ThemeError::NvChad("unterminated Lua block comment".into()));
            }
            index += 2;
            blank_comment(&mut result[start..index]);
        } else if bytes[index..].starts_with(b"--") {
            let start = index;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            blank_comment(&mut result[start..index]);
        } else {
            index += 1;
        }
    }
    if quote.is_some() {
        return Err(ThemeError::NvChad("unterminated Lua string".into()));
    }
    String::from_utf8(result).map_err(|_| ThemeError::InvalidUtf8)
}

fn blank_comment(bytes: &mut [u8]) {
    for byte in bytes {
        if *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
}

fn find_lua_assignment<'a>(source: &'a str, field: &str) -> Result<&'a str, ThemeError> {
    let bytes = source.as_bytes();
    let mut offset = 0;
    let mut quote = None;
    while offset < bytes.len() {
        if let Some(delimiter) = quote {
            if bytes[offset] == b'\\' {
                offset = (offset + 2).min(bytes.len());
            } else {
                if bytes[offset] == delimiter {
                    quote = None;
                }
                offset += 1;
            }
            continue;
        }
        if matches!(bytes[offset], b'\'' | b'"') {
            quote = Some(bytes[offset]);
            offset += 1;
            continue;
        }
        if !bytes[offset..].starts_with(field.as_bytes()) {
            offset += 1;
            continue;
        }
        let start = offset;
        let before_is_identifier = start > 0
            && (source.as_bytes()[start - 1].is_ascii_alphanumeric()
                || matches!(source.as_bytes()[start - 1], b'_' | b'.'));
        let after = start + field.len();
        let after_is_identifier = source
            .as_bytes()
            .get(after)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        if !before_is_identifier && !after_is_identifier {
            let remainder = source[after..].trim_start();
            if let Some(value) = remainder.strip_prefix('=') {
                return Ok(value.trim_start());
            }
        }
        offset = after;
    }
    Err(ThemeError::NvChad(format!("missing {field} assignment")))
}

fn extract_lua_table(source: &str, field: &str) -> Result<String, ThemeError> {
    let value = find_lua_assignment(source, field)?;
    if !value.starts_with('{') {
        return Err(ThemeError::NvChad(format!(
            "{field} must be a literal table"
        )));
    }
    let bytes = value.as_bytes();
    let mut depth = 0_u32;
    let mut quote = None;
    let mut index = 0;
    while index < bytes.len() {
        if let Some(delimiter) = quote {
            if bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
                continue;
            }
            if bytes[index] == delimiter {
                quote = None;
            }
        } else if matches!(bytes[index], b'\'' | b'"') {
            quote = Some(bytes[index]);
        } else if bytes[index] == b'{' {
            depth += 1;
        } else if bytes[index] == b'}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Ok(value[1..index].to_owned());
            }
        }
        index += 1;
    }
    Err(ThemeError::NvChad(format!("unterminated {field} table")))
}

fn extract_lua_string_assignment(source: &str, field: &str) -> Result<String, ThemeError> {
    let value = find_lua_assignment(source, field)?;
    let (parsed, consumed) = parse_lua_string(value, 0)?;
    let remainder = value[consumed..].trim_start_matches([' ', '\t']);
    if !remainder.is_empty() && !remainder.starts_with(['\n', '\r', ';']) {
        return Err(ThemeError::NvChad(format!(
            "{field} must be a literal string"
        )));
    }
    Ok(parsed)
}

fn parse_local_colors(source: &str) -> BTreeMap<String, String> {
    let mut colors = BTreeMap::new();
    for line in source.lines() {
        let Some(declaration) = line.trim().strip_prefix("local ") else {
            continue;
        };
        let Some((name, value)) = declaration.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            continue;
        }
        let value = value.trim();
        if let Ok((color, consumed)) = parse_lua_string(value, 0) {
            if value[consumed..].trim().is_empty() && is_opaque_hex(&color) {
                colors.insert(name.to_owned(), color);
            }
        }
    }
    colors
}

fn parse_lua_color_table(
    body: &str,
    base_30: Option<&BTreeMap<String, String>>,
    local_colors: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, ThemeError> {
    let mut colors = BTreeMap::new();
    let mut index = 0;
    while index < body.len() {
        index = skip_lua_spacing(body, index);
        if index == body.len() {
            break;
        }
        let (key, next) = parse_lua_key(body, index)?;
        index = skip_ascii_whitespace(body, next);
        if body.as_bytes().get(index) != Some(&b'=') {
            return Err(ThemeError::NvChad(format!(
                "palette entry {key} must use ="
            )));
        }
        index = skip_ascii_whitespace(body, index + 1);
        let (color, next) = if body
            .as_bytes()
            .get(index)
            .is_some_and(|byte| matches!(byte, b'\'' | b'"'))
        {
            parse_lua_string(body, index)?
        } else if let Some(remainder) = body[index..].strip_prefix("M.base_30.") {
            let references = base_30.ok_or_else(|| {
                ThemeError::NvChad(format!(
                    "palette entry {key} cannot reference another palette"
                ))
            })?;
            let (referenced_key, consumed) = parse_lua_key(remainder, 0)?;
            let color = references.get(&referenced_key).ok_or_else(|| {
                ThemeError::NvChad(format!(
                    "palette entry {key} references missing M.base_30.{referenced_key}"
                ))
            })?;
            (color.clone(), index + "M.base_30.".len() + consumed)
        } else {
            let (name, consumed) = parse_lua_key(body, index)?;
            let color = local_colors.get(&name).ok_or_else(|| {
                ThemeError::NvChad(format!(
                    "palette entry {key} references unknown local color {name}"
                ))
            })?;
            (color.clone(), consumed)
        };
        if !is_opaque_hex(&color) {
            return Err(ThemeError::NvChad(format!(
                "palette entry {key} must be a literal #RRGGBB color"
            )));
        }
        if colors.insert(key.clone(), color).is_some() {
            return Err(ThemeError::NvChad(format!(
                "palette entry {key} is duplicated"
            )));
        }
        index = skip_ascii_whitespace(body, next);
        match body.as_bytes().get(index) {
            Some(b',' | b';') => index += 1,
            None => break,
            _ => {
                return Err(ThemeError::NvChad(format!(
                    "palette entry {key} must end with a comma"
                )))
            }
        }
    }
    Ok(colors)
}

fn skip_lua_spacing(source: &str, mut index: usize) -> usize {
    while let Some(byte) = source.as_bytes().get(index) {
        if byte.is_ascii_whitespace() || matches!(byte, b',' | b';') {
            index += 1;
        } else {
            break;
        }
    }
    index
}

fn skip_ascii_whitespace(source: &str, mut index: usize) -> usize {
    while source
        .as_bytes()
        .get(index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        index += 1;
    }
    index
}

fn parse_lua_key(source: &str, index: usize) -> Result<(String, usize), ThemeError> {
    if source.as_bytes().get(index) == Some(&b'[') {
        let start = skip_ascii_whitespace(source, index + 1);
        let (key, next) = parse_lua_string(source, start)?;
        let next = skip_ascii_whitespace(source, next);
        if source.as_bytes().get(next) != Some(&b']') {
            return Err(ThemeError::NvChad("invalid bracketed palette key".into()));
        }
        return Ok((key, next + 1));
    }
    let mut end = index;
    while source
        .as_bytes()
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        end += 1;
    }
    if end == index {
        return Err(ThemeError::NvChad("invalid palette key".into()));
    }
    Ok((source[index..end].to_owned(), end))
}

fn parse_lua_string(source: &str, index: usize) -> Result<(String, usize), ThemeError> {
    let Some(&quote) = source.as_bytes().get(index) else {
        return Err(ThemeError::NvChad("expected a literal string".into()));
    };
    if !matches!(quote, b'\'' | b'"') {
        return Err(ThemeError::NvChad("expected a literal string".into()));
    }
    let mut end = index + 1;
    while let Some(&byte) = source.as_bytes().get(end) {
        if byte == b'\\' {
            return Err(ThemeError::NvChad(
                "escaped palette strings are not supported".into(),
            ));
        }
        if byte == quote {
            return Ok((source[index + 1..end].to_owned(), end + 1));
        }
        end += 1;
    }
    Err(ThemeError::NvChad("unterminated literal string".into()))
}

fn is_opaque_hex(color: &str) -> bool {
    color.len() == 7
        && color.starts_with('#')
        && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn with_alpha(color: &str, alpha: &str) -> String {
    format!("{color}{alpha}")
}

fn mix_hex(background: &str, foreground: &str, weight: u16) -> Result<String, ThemeError> {
    let parse = |color: &str, start: usize| {
        u16::from_str_radix(&color[start..start + 2], 16)
            .map_err(|_| ThemeError::NvChad("invalid palette color".into()))
    };
    let mix = |background: u16, foreground: u16| {
        (background * (100 - weight) + foreground * weight + 50) / 100
    };
    Ok(format!(
        "#{:02X}{:02X}{:02X}",
        mix(parse(background, 1)?, parse(foreground, 1)?),
        mix(parse(background, 3)?, parse(foreground, 3)?),
        mix(parse(background, 5)?, parse(foreground, 5)?)
    ))
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

fn display_name(value: &str) -> String {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                format!("{}{}", first.to_ascii_uppercase(), characters.as_str())
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_id(id: &str) -> Result<(), ThemeError> {
    if id.is_empty()
        || id.len() > 128
        || !id.contains('.')
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        Err(ThemeError::InvalidId(id.into()))
    } else {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("invalid theme ID: {0}")]
    InvalidId(String),
    #[error("required theme token is missing: {0}")]
    MissingToken(String),
    #[error("theme token {token} has invalid color {color}")]
    InvalidColor { token: String, color: String },
    #[error("unknown theme token: {0}")]
    UnknownToken(String),
    #[error("theme metadata is invalid")]
    InvalidMetadata,
    #[error("theme file is not a bounded regular file")]
    InvalidFile,
    #[error("theme files must be .nulis-theme.json or NvChad/Base46 .lua files")]
    UnsupportedFormat,
    #[error("theme source is not valid UTF-8")]
    InvalidUtf8,
    #[error("NvChad/Base46 theme is not declarative or supported: {0}")]
    NvChad(String),
    #[error("theme ID is already installed: {0}")]
    Duplicate(String),
    #[error("built-in theme cannot be changed: {0}")]
    BuiltIn(String),
    #[error("theme was not found: {0}")]
    NotFound(String),
    #[error("the user-theme limit was reached")]
    TooManyThemes,
    #[error("theme I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("theme JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::{
        import_nvchad_theme, Theme, ThemeCatalog, ThemeKind, OPTIONAL_THEME_TOKENS, THEME_TOKENS,
    };

    const BASE46_THEME: &str = r##"
local M = {}
M.base_30 = {
  black = "#101317", -- editor background
  white = '#D4D4D5',
  blue = "#7AB0DF",
  green = "#37D99E",
  red = "#F87070",
}
M.base_16 = {
  base00 = "#101317", base01 = "#1A1D21", base02 = "#23262A",
  base03 = "#2B2E32", base04 = "#323539", base05 = "#C5C5C6",
  base06 = "#CBCBCC", base07 = "#D4D4D5", base08 = "#37D99E",
  base09 = "#F0A988", base0A = "#E5D487", base0B = "#E87979",
  base0C = "#37D99E", base0D = "#5FB0FC", base0E = "#C397D8",
  base0F = "#E87979",
}
M.type = "dark"
M = require("base46").override_theme(M, "radium")
return M
"##;

    #[test]
    fn rejects_non_hex_color_values() {
        let mut theme = Theme {
            schema: None,
            id: "nulis.test".into(),
            name: "Test".into(),
            version: 1,
            kind: ThemeKind::Dark,
            colors: THEME_TOKENS
                .iter()
                .map(|token| ((*token).into(), "#000000".into()))
                .collect(),
        };
        theme
            .colors
            .insert("editor.foreground".into(), "white".into());
        assert!(theme.validate().is_err());
    }

    #[test]
    fn rejects_unknown_tokens() {
        let mut theme = Theme {
            schema: None,
            id: "nulis.test".into(),
            name: "Test".into(),
            version: 1,
            kind: ThemeKind::Dark,
            colors: THEME_TOKENS
                .iter()
                .map(|token| ((*token).into(), "#000000".into()))
                .collect(),
        };
        theme
            .colors
            .insert("ui.arbitraryCss".into(), "#000000".into());
        assert!(theme.validate().is_err());
    }

    #[test]
    fn catalog_installs_and_uninstalls_user_theme() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source = directory.path().join("source.nulis-theme.json");
        let mut theme = Theme {
            schema: None,
            id: "example.ocean".into(),
            name: "Ocean".into(),
            version: 1,
            kind: ThemeKind::Dark,
            colors: THEME_TOKENS
                .iter()
                .map(|token| ((*token).into(), "#123456".into()))
                .collect(),
        };
        theme.colors.insert("ui.overlay".into(), "#123456AA".into());
        std::fs::write(&source, serde_json::to_vec(&theme).expect("serialize")).expect("write");
        let catalog = ThemeCatalog::new(directory.path().join("themes"));
        assert_eq!(
            catalog.install(&source).expect("install").theme.id,
            "example.ocean"
        );
        let listed = catalog.list().expect("list");
        assert_eq!(listed.len(), 100);
        // Sorted by name, which puts Nulis's own themes first.
        let names: Vec<_> = listed
            .iter()
            .map(|record| record.theme.name.as_str())
            .collect();
        assert_eq!(
            &names[..2],
            ["Nulis · Ember Night", "Nulis · Paper Current"]
        );
        assert!(names[2..].windows(2).all(|pair| pair[0] <= pair[1]));
        catalog.uninstall("example.ocean").expect("uninstall");
        assert_eq!(catalog.list().expect("list").len(), 99);
    }

    #[test]
    fn imports_literal_base46_palette_and_persists_json() {
        let converted = import_nvchad_theme(BASE46_THEME, "sample-radium").expect("convert");
        assert_eq!(converted.id, "nvchad.sample-radium");
        assert_eq!(converted.kind, ThemeKind::Dark);
        assert_eq!(converted.colors["editor.background"], "#101317");
        assert_eq!(converted.colors["ui.accent"], "#7AB0DF");
        assert_eq!(
            converted.colors.len(),
            THEME_TOKENS.len() + OPTIONAL_THEME_TOKENS.len()
        );

        let directory = tempfile::tempdir().expect("temporary directory");
        let source = directory.path().join("sample-radium.lua");
        std::fs::write(&source, BASE46_THEME).expect("write source");
        let catalog = ThemeCatalog::new(directory.path().join("themes"));
        assert_eq!(catalog.install(&source).expect("install").theme, converted);
        assert!(catalog
            .list()
            .expect("list")
            .iter()
            .any(|item| item.theme == converted));
        assert!(catalog.install(&source).is_err());
        catalog
            .uninstall("nvchad.sample-radium")
            .expect("uninstall");
    }

    #[test]
    fn bundled_base46_catalog_matches_pinned_sources() {
        let sources =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/nvchad-source");
        let catalog: Vec<Theme> =
            serde_json::from_str(include_str!("../../../themes/nvchad-catalog.json"))
                .expect("bundled catalog");
        let mut generated = std::fs::read_dir(sources)
            .expect("source directory")
            .map(|entry| entry.expect("source entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "lua"))
            .collect::<Vec<_>>();
        generated.sort();
        assert_eq!(generated.len(), 97);
        assert_eq!(catalog.len(), generated.len());
        for (theme, path) in catalog.iter().zip(generated) {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("filename");
            let source = std::fs::read_to_string(&path).expect("source text");
            assert_eq!(*theme, import_nvchad_theme(&source, name).expect("convert"));
        }
        let directory = tempfile::tempdir().expect("temporary directory");
        let listed = ThemeCatalog::new(directory.path().join("themes"))
            .list()
            .expect("built-ins");
        assert_eq!(listed.len(), 99);
        assert!(listed.iter().all(|item| item.built_in));
    }

    #[test]
    fn rejects_dynamic_or_incomplete_base46_palettes() {
        assert!(import_nvchad_theme(
            &BASE46_THEME.replace("base00 = \"#101317\"", "base00 = palette.black"),
            "dynamic"
        )
        .is_err());
        assert!(import_nvchad_theme(
            &BASE46_THEME.replace("base0F = \"#E87979\",", ""),
            "incomplete"
        )
        .is_err());
        assert!(import_nvchad_theme(
            &BASE46_THEME.replace("M.type = \"dark\"", "M.type = \"dark\" .. suffix"),
            "dynamic-type"
        )
        .is_err());
        assert!(import_nvchad_theme(
            &BASE46_THEME.replace("M.type = \"dark\"", "local text = 'M.type = \"dark\"'"),
            "quoted-type"
        )
        .is_err());
    }
}
