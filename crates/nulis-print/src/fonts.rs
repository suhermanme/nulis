//! Fonts for printed pages: the bundled `JetBrains Mono`, and installed fonts
//! for the characters it lacks (Chinese, Japanese, Korean, many symbols).
//!
//! The bundled fonts are embedded whole. A fallback font is embedded as a
//! subset holding only the glyphs used, because system fonts with large
//! character sets are tens of megabytes. Only fonts with TrueType outlines
//! are used as fallbacks: they embed as `FontFile2` with the same glyph
//! numbering, which `subsetter` keeps. Color fonts (emoji) are skipped.
//! Text is drawn one glyph per character without shaping, so scripts that
//! need shaping (Arabic, Indic, Javanese) do not print correctly.

use crate::{pdf::ObjectSink, PrintError};
use pdf_writer::{
    types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap},
    Chunk, Filter, Name, Rect, Ref, Str,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Mutex, OnceLock},
};

pub(crate) const REGULAR_TTF: &[u8] = include_bytes!(
    "../../../apps/desktop/src/assets/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf"
);
pub(crate) const BOLD_TTF: &[u8] =
    include_bytes!("../../../apps/desktop/src/assets/fonts/jetbrains-mono/JetBrainsMono-Bold.ttf");

/// Whether the bundled regular font has a glyph for `character`.
pub(crate) fn bundled_has_glyph(character: char) -> bool {
    static FACE: OnceLock<Option<ttf_parser::Face<'static>>> = OnceLock::new();
    FACE.get_or_init(|| ttf_parser::Face::parse(REGULAR_TTF, 0).ok())
        .as_ref()
        .is_some_and(|face| face.glyph_index(character).is_some())
}

/// Most fallback fonts one PDF embeds.
const MAX_FALLBACK_FONTS: usize = 16;

const SYSTEM_INFO: SystemInfo = SystemInfo {
    registry: Str(b"Adobe"),
    ordering: Str(b"Identity"),
    supplement: 0,
};

/// The five objects of one embedded font.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FontRefs {
    pub(crate) type0: Ref,
    cid: Ref,
    descriptor: Ref,
    file: Ref,
    to_unicode: Ref,
}

impl FontRefs {
    pub(crate) const fn starting_at(first: i32) -> Self {
        Self {
            type0: Ref::new(first),
            cid: Ref::new(first + 1),
            descriptor: Ref::new(first + 2),
            file: Ref::new(first + 3),
            to_unicode: Ref::new(first + 4),
        }
    }
}

/// Hands out object numbers for fonts added while writing.
pub(crate) trait RefSource {
    fn allocate(&mut self) -> Result<Ref, PrintError>;
}

/// Metrics of the bundled font, in em.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub advance: f32,
    pub ascender: f32,
    pub descender: f32,
}

#[derive(Clone, Copy, Debug)]
enum FontData {
    Bundled(&'static [u8]),
    System(fontdb::ID),
}

/// A glyph placed from some font: which font, its number, and its width in em.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placed {
    pub(crate) font: usize,
    pub(crate) glyph: u16,
    pub(crate) advance: f32,
}

#[derive(Debug)]
pub(crate) struct EmbeddedFont {
    data: FontData,
    name: String,
    refs: FontRefs,
    /// Resource name used on pages: `F1`, `F2`, …
    pub(crate) resource: String,
    monospace: bool,
    units: f32,
    ascender: i16,
    descender: i16,
    cap_height: i16,
    bbox: ttf_parser::Rect,
    advance: u16,
    glyphs: HashMap<char, Option<(u16, u16)>>,
    /// Glyph to (character, advance), for widths and copying text out.
    used: BTreeMap<u16, (char, u16)>,
}

impl EmbeddedFont {
    fn from_face(
        data: FontData,
        name: String,
        refs: FontRefs,
        resource: String,
        face: &ttf_parser::Face<'_>,
        monospace: bool,
    ) -> Self {
        let advance = face
            .glyph_index('0')
            .and_then(|glyph| face.glyph_hor_advance(glyph))
            .unwrap_or(face.units_per_em());
        Self {
            data,
            name,
            refs,
            resource,
            monospace,
            units: f32::from(face.units_per_em()),
            ascender: face.ascender(),
            descender: face.descender(),
            cap_height: face.capital_height().unwrap_or(face.ascender()),
            bbox: face.global_bounding_box(),
            advance,
            glyphs: HashMap::new(),
            used: BTreeMap::new(),
        }
    }

    pub(crate) fn bundled(
        data: &'static [u8],
        name: &str,
        refs: FontRefs,
        resource: &str,
    ) -> Result<Self, PrintError> {
        let face = ttf_parser::Face::parse(data, 0)
            .map_err(|error| PrintError::Font(format!("{name}: {error}")))?;
        Ok(Self::from_face(
            FontData::Bundled(data),
            name.into(),
            refs,
            resource.into(),
            &face,
            true,
        ))
    }

    fn with_face<T>(&self, use_face: impl FnOnce(&ttf_parser::Face<'_>) -> T) -> Option<T> {
        match self.data {
            FontData::Bundled(data) => ttf_parser::Face::parse(data, 0)
                .ok()
                .map(|face| use_face(&face)),
            FontData::System(id) => system_fonts()
                .db
                .with_face_data(id, |data, index| {
                    ttf_parser::Face::parse(data, index)
                        .ok()
                        .map(|face| use_face(&face))
                })
                .flatten(),
        }
    }

    pub(crate) fn refs_type0(&self) -> Ref {
        self.refs.type0
    }

    pub(crate) fn metrics(&self) -> FontMetrics {
        FontMetrics {
            advance: f32::from(self.advance) / self.units,
            ascender: f32::from(self.ascender) / self.units,
            descender: f32::from(self.descender) / self.units,
        }
    }

    /// The glyph and its advance (in font units) for `character`, if the font has one.
    fn lookup(&mut self, character: char) -> Option<(u16, u16)> {
        if let Some(known) = self.glyphs.get(&character) {
            return *known;
        }
        let found = self
            .with_face(|face| {
                let glyph = face.glyph_index(character)?;
                Some((glyph.0, face.glyph_hor_advance(glyph).unwrap_or(0)))
            })
            .flatten()
            .filter(|(glyph, _)| *glyph != 0);
        self.glyphs.insert(character, found);
        if let Some((glyph, advance)) = found {
            self.used.entry(glyph).or_insert((character, advance));
        }
        found
    }

    /// Writes the font objects: a Type 0 font over a CID TrueType font, the
    /// font file (compressed; subset for a fallback font), and the map from
    /// glyphs back to text.
    pub(crate) fn write(&self, chunks: &mut ObjectSink<'_>) -> Result<(), PrintError> {
        let file = self.font_file()?;
        let scale = |value: i16| f32::from(value) * 1000.0 / self.units;
        let name = Name(self.name.as_bytes());

        let mut chunk = Chunk::new();
        chunk
            .type0_font(self.refs.type0)
            .base_font(name)
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(self.refs.cid)
            .to_unicode(self.refs.to_unicode);
        chunks.write(&chunk)?;

        let mut chunk = Chunk::new();
        let mut cid = chunk.cid_font(self.refs.cid);
        cid.subtype(CidFontType::Type2)
            .base_font(name)
            .system_info(SYSTEM_INFO)
            .font_descriptor(self.refs.descriptor)
            .default_width(f32::from(self.advance) * 1000.0 / self.units)
            .cid_to_gid_map_predefined(Name(b"Identity"));
        if !self.monospace {
            let mut widths = cid.widths();
            for (glyph, (_, advance)) in &self.used {
                widths.consecutive(*glyph, [f32::from(*advance) * 1000.0 / self.units]);
            }
        }
        drop(cid);
        chunks.write(&chunk)?;

        let flags = if self.monospace {
            FontFlags::FIXED_PITCH | FontFlags::SYMBOLIC
        } else {
            FontFlags::SYMBOLIC
        };
        let mut chunk = Chunk::new();
        chunk
            .font_descriptor(self.refs.descriptor)
            .name(name)
            .flags(flags)
            .bbox(Rect::new(
                scale(self.bbox.x_min),
                scale(self.bbox.y_min),
                scale(self.bbox.x_max),
                scale(self.bbox.y_max),
            ))
            .italic_angle(0.0)
            .ascent(scale(self.ascender))
            .descent(scale(self.descender))
            .cap_height(scale(self.cap_height))
            .stem_v(80.0)
            .font_file2(self.refs.file);
        chunks.write(&chunk)?;

        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&file, 6);
        let mut chunk = Chunk::new();
        chunk
            .stream(self.refs.file, &compressed)
            .filter(Filter::FlateDecode)
            .pair(
                Name(b"Length1"),
                i32::try_from(file.len())
                    .map_err(|_| PrintError::Font("font file is too large".into()))?,
            );
        chunks.write(&chunk)?;

        let mut cmap = UnicodeCmap::new(Name(b"Nulis-UCS"), SYSTEM_INFO);
        for (glyph, (character, _)) in &self.used {
            cmap.pair(*glyph, *character);
        }
        let cmap = cmap.finish();
        let mut chunk = Chunk::new();
        chunk
            .cmap(self.refs.to_unicode, &cmap)
            .system_info(SYSTEM_INFO);
        chunks.write(&chunk)
    }

    fn font_file(&self) -> Result<Vec<u8>, PrintError> {
        match self.data {
            FontData::Bundled(data) => Ok(data.to_vec()),
            FontData::System(id) => {
                let mut glyphs: Vec<u16> = self.used.keys().copied().collect();
                glyphs.insert(0, 0);
                system_fonts()
                    .db
                    .with_face_data(id, |data, index| {
                        subsetter::subset(data, index, subsetter::Profile::pdf(&glyphs))
                    })
                    .ok_or_else(|| {
                        PrintError::Font(format!("{} is no longer installed", self.name))
                    })?
                    .map_err(|error| PrintError::Font(format!("{}: {error:?}", self.name)))
            }
        }
    }
}

/// Installed fonts that can stand in for the bundled one, found once per run.
struct SystemFonts {
    db: fontdb::Database,
    /// TrueType-outline faces, monospaced and regular weight first.
    candidates: Vec<(fontdb::ID, String)>,
    /// Which candidate covers a character, remembered across exports.
    coverage: Mutex<HashMap<char, Option<usize>>>,
}

fn system_fonts() -> &'static SystemFonts {
    static FONTS: OnceLock<SystemFonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let mut candidates: Vec<_> = db
            .faces()
            .filter(|info| {
                db.with_face_data(info.id, |data, index| {
                    ttf_parser::Face::parse(data, index).is_ok_and(|face| {
                        // Outlines only: color fonts (emoji) draw from
                        // bitmaps or layers a PDF glyph cannot show.
                        let tables = face.tables();
                        tables.glyf.is_some()
                            && tables.sbix.is_none()
                            && tables.cbdt.is_none()
                            && tables.colr.is_none()
                            && tables.svg.is_none()
                    })
                })
                .unwrap_or(false)
            })
            .map(|info| {
                let rank = (
                    !info.monospaced,
                    info.weight.0.abs_diff(400),
                    info.style != fontdb::Style::Normal,
                    info.post_script_name.clone(),
                );
                (rank, info.id, info.post_script_name.clone())
            })
            .collect();
        candidates.sort_by(|left, right| left.0.cmp(&right.0));
        SystemFonts {
            candidates: candidates
                .into_iter()
                .map(|(_, id, name)| (id, name))
                .collect(),
            db,
            coverage: Mutex::new(HashMap::new()),
        }
    })
}

impl SystemFonts {
    /// The first candidate with a glyph for `character`.
    fn find(&self, character: char) -> Option<usize> {
        if let Some(known) = self
            .coverage
            .lock()
            .ok()
            .and_then(|coverage| coverage.get(&character).copied())
        {
            return known;
        }
        let found = self.candidates.iter().position(|(id, _)| {
            self.db
                .with_face_data(*id, |data, index| {
                    ttf_parser::Face::parse(data, index)
                        .ok()
                        .and_then(|face| face.glyph_index(character))
                        .is_some_and(|glyph| glyph.0 != 0)
                })
                .unwrap_or(false)
        });
        if let Ok(mut coverage) = self.coverage.lock() {
            coverage.insert(character, found);
        }
        found
    }
}

/// The fonts of one PDF: regular (0), bold (1), then fallbacks as they are needed.
#[derive(Debug)]
pub(crate) struct FontSet {
    fonts: Vec<EmbeddedFont>,
    /// Which candidate each fallback font is.
    fallback_candidates: Vec<usize>,
    /// Whether to look in installed fonts for missing characters.
    fallback: bool,
}

impl FontSet {
    pub(crate) fn new(
        regular: FontRefs,
        bold: FontRefs,
        fallback: bool,
    ) -> Result<Self, PrintError> {
        Ok(Self {
            fonts: vec![
                EmbeddedFont::bundled(REGULAR_TTF, "JetBrainsMono-Regular", regular, "F1")?,
                EmbeddedFont::bundled(BOLD_TTF, "JetBrainsMono-Bold", bold, "F2")?,
            ],
            fallback_candidates: Vec::new(),
            fallback,
        })
    }

    pub(crate) fn font(&self, index: usize) -> &EmbeddedFont {
        &self.fonts[index]
    }

    pub(crate) fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Where to draw `character`: the bundled font of that weight, else an
    /// installed font that has it, else `None` (the cell stays blank).
    ///
    /// # Errors
    /// Returns an error only if a fallback font's objects cannot be numbered.
    pub(crate) fn place(
        &mut self,
        character: char,
        bold: bool,
        refs: &mut dyn RefSource,
    ) -> Result<Option<Placed>, PrintError> {
        let primary = usize::from(bold);
        let placed = |font: usize, (glyph, advance): (u16, u16), units: f32| Placed {
            font,
            glyph,
            advance: f32::from(advance) / units,
        };
        if let Some(found) = self.fonts[primary].lookup(character) {
            return Ok(Some(placed(primary, found, self.fonts[primary].units)));
        }
        if !self.fallback || character.is_whitespace() || character.is_control() {
            return Ok(None);
        }
        let Some(candidate) = system_fonts().find(character) else {
            return Ok(None);
        };
        let index = if let Some(existing) = self
            .fallback_candidates
            .iter()
            .position(|known| *known == candidate)
        {
            existing + 2
        } else {
            if self.fallback_candidates.len() == MAX_FALLBACK_FONTS {
                return Ok(None);
            }
            let first = refs.allocate()?;
            for _ in 0..4 {
                refs.allocate()?;
            }
            let (id, post_script_name) = &system_fonts().candidates[candidate];
            let number = self.fallback_candidates.len();
            let name = subset_name(number, post_script_name);
            let resource = format!("F{}", number + 3);
            let font = system_fonts()
                .db
                .with_face_data(*id, |data, face_index| {
                    ttf_parser::Face::parse(data, face_index).ok().map(|face| {
                        EmbeddedFont::from_face(
                            FontData::System(*id),
                            name,
                            FontRefs::starting_at(first.get()),
                            resource,
                            &face,
                            false,
                        )
                    })
                })
                .flatten();
            let Some(font) = font else {
                return Ok(None);
            };
            self.fonts.push(font);
            self.fallback_candidates.push(candidate);
            self.fonts.len() - 1
        };
        let units = self.fonts[index].units;
        Ok(self.fonts[index]
            .lookup(character)
            .map(|found| placed(index, found, units)))
    }
}

/// A subset font's PDF name: a six-letter tag, `+`, and its PostScript name.
fn subset_name(number: usize, post_script_name: &str) -> String {
    let letter = char::from(b'A' + u8::try_from(number % 26).unwrap_or(0));
    let clean: String = post_script_name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .collect();
    let clean = if clean.is_empty() {
        "Fallback".into()
    } else {
        clean
    };
    format!("NULIS{letter}+{clean}")
}

#[cfg(test)]
mod tests {
    use super::subset_name;

    #[test]
    fn subset_names_have_a_tag_and_a_clean_postscript_name() {
        assert_eq!(subset_name(0, "ArialUnicodeMS"), "NULISA+ArialUnicodeMS");
        assert_eq!(
            subset_name(2, ".Hiragino Sans GB W3"),
            "NULISC+HiraginoSansGBW3"
        );
        assert_eq!(subset_name(1, "∅"), "NULISB+Fallback");
    }
}
