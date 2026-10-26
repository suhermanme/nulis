use super::{
    byte_at_scalar_column, scalar_count, ApplyCompareRequest, BookmarkDirection,
    DocumentCompareRequest, DocumentManager, EditorError, HexViewportRequest, LineChange,
    Selection, SparseNavigationIndex, TextEncoding, TextTransform, VerticalDirection,
    ViewportAtRequest, ViewportLineRequest, ViewportRequest, MAX_DOCUMENT_MAP_SAMPLES,
    MAX_NAVIGATION_CHECKPOINTS,
};
use nulis_diff::CompareMode;
use nulis_document::PieceTable;
use nulis_syntax::{CommentSyntax, LanguageDescriptor, LanguageId, LanguageModule, Vocabulary};
use std::{
    fs::{self, OpenOptions},
    io::{Seek, SeekFrom, Write},
};

fn plugin_language(
    id: &str,
    plugin_id: &str,
    extensions: &[&str],
    association_only: bool,
) -> crate::plugin_languages::PluginLanguage {
    let id = LanguageId::new(id).expect("language id");
    let module = LanguageModule::declarative(
        LanguageDescriptor {
            display_name: id.as_str().to_uppercase(),
            id,
            extensions: extensions.iter().map(|item| (*item).to_owned()).collect(),
            filenames: Vec::new(),
        },
        None,
        CommentSyntax::default(),
        Vocabulary::default(),
        None,
    )
    .expect("declarative module");
    crate::plugin_languages::PluginLanguage {
        plugin_id: plugin_id.into(),
        module,
        association_only,
        providers: Vec::new(),
    }
}

#[test]
fn viewport_is_bounded_and_continues_long_lines() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "abcdefghij\nnext".into())
        .expect("document");
    let first = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 4,
        })
        .expect("viewport");
    assert_eq!(first.lines[0].text, "abcd");
    assert!(first.lines[0].truncated);
    let cursor = first.next.expect("continuation cursor");
    assert_eq!(cursor.line, 1);
    assert!(cursor.line_continuation);
}

#[test]
fn compare_changes_are_bounded_and_merge_as_one_undoable_edit() {
    let manager = DocumentManager::default();
    let left = manager
        .create_untitled("left".into(), "same\nleft\ntail\n".into())
        .expect("left");
    let right = manager
        .create_untitled("right".into(), "same\nright\ntail\n".into())
        .expect("right");
    let page = manager
        .compare_documents(&DocumentCompareRequest {
            left_id: left.document_id.clone(),
            right_id: right.document_id.clone(),
            mode: CompareMode::Text,
            left_start_byte: 0,
            right_start_byte: 0,
            left_start_line: 1,
            right_start_line: 1,
            max_bytes: 1024,
            max_changes: 10,
        })
        .expect("compare");
    assert_eq!(page.changes.len(), 1);
    let change = &page.changes[0];
    let updated = manager
        .apply_compare_change(&ApplyCompareRequest {
            source_id: left.document_id,
            target_id: right.document_id.clone(),
            expected_target_revision: right.revision,
            source_start: change.left.start_byte,
            source_end: change.left.end_byte,
            target_start: change.right.start_byte,
            target_end: change.right.end_byte,
        })
        .expect("merge");
    assert_eq!(updated.revision, 1);
    manager.undo(&right.document_id).expect("undo");
    let restored = manager
        .viewport(&ViewportRequest {
            document_id: right.document_id,
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(restored.lines[1].text, "right");
}

#[test]
fn hex_viewport_is_aligned_bounded_and_byte_native() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("bytes.bin".into(), "0123456789ABCDEFghij".into())
        .expect("document");
    let snapshot = manager
        .hex_viewport(&HexViewportRequest {
            document_id: document.document_id,
            start_byte: 19,
            max_bytes: 16,
            bytes_per_row: 16,
        })
        .expect("hex viewport");
    assert_eq!(snapshot.rows.len(), 1);
    assert_eq!(snapshot.rows[0].start_byte, 16);
    assert_eq!(snapshot.rows[0].bytes, b"ghij");
    assert!(snapshot.next.is_none());
    let serialized = serde_json::to_value(snapshot).expect("serialize hex viewport");
    assert_eq!(serialized["rows"][0]["startByte"], "16");
    assert_eq!(serialized["documentByteLen"], "20");
}

#[test]
fn hex_viewport_reads_a_sparse_file_beyond_four_gibibytes() {
    // A file extended with `set_len` is sparse on Unix file systems. On
    // Windows it is fully allocated unless explicitly marked sparse, so
    // this test would need gigabytes of free disk and time to zero them.
    // The 64-bit offset arithmetic it exercises is the same everywhere.
    if cfg!(windows) {
        return;
    }
    let root = tempfile::tempdir().expect("temporary root");
    let source_path = root.path().join("huge.bin");
    let marker = 5_u64 * 1024 * 1024 * 1024 + 4;
    let mut source = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&source_path)
        .expect("sparse source");
    source.set_len(marker + 16).expect("sparse length");
    source.seek(SeekFrom::Start(marker)).expect("seek marker");
    source.write_all(&[0xde, 0xad, 0xbe, 0xef]).expect("marker");
    source.sync_all().expect("sync marker");

    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&source_path).expect("open sparse source");
    let snapshot = manager
        .hex_viewport(&HexViewportRequest {
            document_id: document.document_id,
            start_byte: marker,
            max_bytes: 16,
            bytes_per_row: 16,
        })
        .expect("deep hex viewport");
    let row = &snapshot.rows[0];
    let relative = usize::try_from(marker - row.start_byte).expect("marker index");
    assert_eq!(&row.bytes[relative..relative + 4], [0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(snapshot.rows.len(), 1);
}

#[test]
fn hexadecimal_nibbles_edit_raw_bytes_and_are_undoable() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("bytes.bin".into(), "\u{12}4".into())
        .expect("document");
    let high = manager
        .type_hex_nibble(&document.document_id, 0, true, 0x0a, true)
        .expect("high nibble");
    assert_eq!(high.caret.byte_offset, 0);
    assert!(!high.caret.high_nibble);
    let low = manager
        .type_hex_nibble(&document.document_id, 1, false, 0x0f, true)
        .expect("low nibble");
    assert_eq!(low.caret.byte_offset, 1);
    assert!(low.caret.high_nibble);
    let snapshot = manager
        .hex_viewport(&HexViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            max_bytes: 16,
            bytes_per_row: 16,
        })
        .expect("edited bytes");
    assert_eq!(snapshot.rows[0].bytes, [0xaf, b'4']);
    manager
        .undo(&document.document_id)
        .expect("undo low nibble");
    manager
        .undo(&document.document_id)
        .expect("undo high nibble");
    let restored = manager
        .hex_viewport(&HexViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            max_bytes: 16,
            bytes_per_row: 16,
        })
        .expect("restored bytes");
    assert_eq!(restored.rows[0].bytes, [0x12, b'4']);
}

#[test]
fn hexadecimal_insert_mode_creates_one_byte_then_completes_its_low_nibble() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("bytes.bin".into(), "A".into())
        .expect("document");
    manager
        .type_hex_nibble(&document.document_id, 0, true, 0x0d, false)
        .expect("insert high nibble");
    manager
        .type_hex_nibble(&document.document_id, 1, false, 0x0e, false)
        .expect("complete low nibble");
    let snapshot = manager
        .hex_viewport(&HexViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            max_bytes: 16,
            bytes_per_row: 16,
        })
        .expect("inserted bytes");
    assert_eq!(snapshot.rows[0].bytes, [0xde, b'A']);
}

#[test]
fn hex_replace_and_clipboard_use_the_authoritative_byte_selection() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("bytes.bin".into(), "abcdef".into())
        .expect("document");
    manager
        .set_selections(
            &document.document_id,
            vec![Selection {
                anchor_byte: 1,
                active_byte: 4,
            }],
        )
        .expect("selection");
    let clipboard = manager
        .hex_selection(&document.document_id)
        .expect("clipboard");
    assert_eq!(clipboard.hex, "62 63 64");
    let result = manager
        .replace_hex_bytes(&document.document_id, 0, "DE AD", true)
        .expect("replace selected bytes");
    assert_eq!(result.document.byte_len, 5);
    let snapshot = manager
        .hex_viewport(&HexViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            max_bytes: 16,
            bytes_per_row: 16,
        })
        .expect("replacement viewport");
    assert_eq!(snapshot.rows[0].bytes, [b'a', 0xde, 0xad, b'e', b'f']);
}

#[test]
fn stale_revision_cannot_overwrite_a_newer_edit() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "abc".into())
        .expect("document");
    manager
        .replace(&document.document_id, 0, 1, 2, "X")
        .expect("first edit");
    assert!(matches!(
        manager.replace(&document.document_id, 0, 0, 0, "!"),
        Err(EditorError::StaleRevision { .. })
    ));
}

#[test]
fn selection_model_accepts_multiple_independent_ranges() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "abcdef".into())
        .expect("document");
    let selections = vec![
        Selection {
            anchor_byte: 1,
            active_byte: 2,
        },
        Selection {
            anchor_byte: 5,
            active_byte: 4,
        },
    ];
    assert_eq!(
        manager
            .set_selections(&document.document_id, selections.clone())
            .expect("selections"),
        selections
    );
}

#[test]
fn multicaret_insert_is_one_revision_and_one_undo_action() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "one two three".into())
        .expect("document");
    manager
        .set_selections(
            &document.document_id,
            vec![
                Selection::caret(0),
                Selection::caret(4),
                Selection::caret(8),
            ],
        )
        .expect("carets");

    let updated = manager
        .insert_text(&document.document_id, 0, "*", false)
        .expect("multicaret insert");
    assert_eq!(updated.revision, 1);
    let snapshot = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(snapshot.lines[0].text, "*one *two *three");
    assert_eq!(
        snapshot.selections,
        vec![
            Selection::caret(1),
            Selection::caret(6),
            Selection::caret(11)
        ]
    );

    manager.undo(&document.document_id).expect("single undo");
    let restored = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("restored viewport");
    assert_eq!(restored.lines[0].text, "one two three");
}

#[test]
fn rectangular_selection_is_constant_size_and_edits_line_local_ranges_atomically() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "alpha\nxy\n12345".into())
        .expect("document");
    let rectangle = manager
        .set_rectangular_selection(&document.document_id, 1, 12, 1, 3)
        .expect("rectangle");
    assert_eq!(rectangle.anchor_line, 1);
    assert_eq!(rectangle.active_line, 3);
    assert_eq!(rectangle.anchor_column, 1);
    assert_eq!(rectangle.active_column, 3);

    let snapshot = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(snapshot.selections.len(), 1);
    assert_eq!(snapshot.rectangular_selection, Some(rectangle));
    assert_eq!(
        manager
            .selection_text(&document.document_id)
            .expect("rectangle text")
            .text,
        "lp\ny\n23"
    );

    let edited = manager
        .insert_text(&document.document_id, 0, "_", false)
        .expect("rectangular insert");
    assert_eq!(edited.revision, 1);
    let edited_snapshot = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("edited viewport");
    assert_eq!(
        edited_snapshot
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        vec!["a_ha", "x_", "1_45"]
    );
    assert!(edited_snapshot.rectangular_selection.is_some());

    manager.undo(&document.document_id).expect("single undo");
    let restored = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("restored viewport");
    assert_eq!(restored.lines[0].text, "alpha");
    assert_eq!(restored.lines[1].text, "xy");
    assert_eq!(restored.lines[2].text, "12345");

    manager
        .set_rectangular_selection(&document.document_id, 1, 12, 1, 3)
        .expect("rectangle for row paste");
    manager
        .insert_text(&document.document_id, restored.revision, "X\nY\nZ", false)
        .expect("row-distributed paste");
    let pasted = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("pasted viewport");
    assert_eq!(pasted.lines[0].text, "aXha");
    assert_eq!(pasted.lines[1].text, "xY");
    assert_eq!(pasted.lines[2].text, "1Z45");
}

#[test]
fn column_selection_survives_typing_and_backspace() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "alpha\nxy\n12345".into())
        .expect("document");
    let id = &document.document_id;
    let column_of = || {
        manager
            .viewport(&ViewportRequest {
                document_id: id.clone(),
                start_byte: 0,
                start_line: 1,
                line_continuation: false,
                max_bytes: 1024,
            })
            .expect("viewport")
            .rectangular_selection
            .expect("column kept")
    };
    manager
        .set_rectangular_selection(id, 1, 12, 1, 3)
        .expect("rectangle");
    // The selection collapses to a column after the inserted text, so
    // typing continues on every row and Backspace removes it again.
    let first = manager.insert_text(id, 0, "_", false).expect("insert");
    let column = column_of();
    assert_eq!((column.anchor_column, column.active_column), (2, 2));
    assert_eq!((column.anchor_line, column.active_line), (1, 3));
    assert_eq!((column.anchor_byte, column.active_byte), (2, 10));
    let second = manager
        .insert_text(id, first.revision, "é!", false)
        .expect("second keystroke");
    assert_eq!(document_text(&manager, id), "a_é!ha\nx_é!\n1_é!45");
    let erased = manager
        .delete_backward(id, second.revision)
        .expect("backspace");
    assert_eq!(document_text(&manager, id), "a_éha\nx_é\n1_é45");
    let column = column_of();
    assert_eq!((column.anchor_column, column.active_column), (3, 3));
    assert_eq!(erased.revision, 3);
    // Moving the caret leaves column mode.
    manager
        .set_selections(id, vec![Selection::caret(0)])
        .expect("caret");
    manager
        .insert_text(id, erased.revision, "#", false)
        .expect("ordinary insert");
    assert_eq!(document_text(&manager, id), "#a_éha\nx_é\n1_é45");
}

#[test]
fn rectangular_insert_pads_virtual_space_without_expanding_selection_state() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "abcd\nx".into())
        .expect("document");
    manager
        .set_rectangular_selection(&document.document_id, 2, 6, 2, 4)
        .expect("rectangle with a short row");
    assert_eq!(
        manager
            .selection_text(&document.document_id)
            .expect("short-row clipboard")
            .text,
        "cd\n"
    );
    manager
        .set_rectangular_selection(&document.document_id, 4, 6, 4, 4)
        .expect("virtual rectangle");
    manager
        .insert_text(&document.document_id, 0, "!", false)
        .expect("virtual insert");
    let snapshot = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(snapshot.lines[0].text, "abcd!");
    assert_eq!(snapshot.lines[1].text, "x   !");
    assert_eq!(snapshot.selections.len(), 1);
}

#[test]
fn viewport_syntax_is_detected_bounded_and_overridable() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled(
            "settings.json".into(),
            "{\n  \"theme\": \"ember\",\n  \"size\": 14\n}".into(),
        )
        .expect("document");
    let snapshot = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("syntax viewport");
    assert_eq!(snapshot.syntax.language_id, "json");
    assert_ne!(snapshot.syntax.highlights.len(), 0);
    assert_eq!(snapshot.syntax.folds.len(), 1);
    assert_eq!(snapshot.syntax.symbols.len(), 2);
    assert_eq!(snapshot.syntax.bracket_pairs.len(), 1);
    let serialized = serde_json::to_value(&snapshot).expect("serialize syntax viewport");
    assert!(serialized["syntax"]["highlights"][0]["startByte"].is_string());

    assert_eq!(
        manager
            .set_language(&document.document_id, "plaintext")
            .expect("language override"),
        "plaintext"
    );
    let plain = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("plain viewport");
    assert_eq!(plain.syntax.language_id, "plaintext");
    assert_eq!(plain.syntax.highlights.len(), 0);
}

#[test]
fn document_map_covers_small_files_and_bounds_large_files() {
    let manager = DocumentManager::default();
    let empty = manager
        .create_untitled("Untitled".into(), String::new())
        .expect("empty document");
    let empty_map = manager.document_map(&empty.document_id).expect("empty map");
    assert_eq!(empty_map.total_lines, Some(1));
    assert_eq!(empty_map.samples.len(), 1);
    assert_eq!(empty_map.samples[0].line, 1);
    let small = manager
        .create_untitled("README.md".into(), "# Title\nbody\n".into())
        .expect("small document");
    let map = manager.document_map(&small.document_id).expect("small map");
    assert_eq!(map.document_byte_len, 13);
    assert_eq!(map.total_lines, Some(3));
    assert_eq!(map.samples[0].text, "# Title");
    assert_eq!(map.samples[1].byte_offset, 8);
    assert_eq!(map.samples[1].line, 2);

    // Long files below the full-read limit keep exact line numbers.
    let long = manager
        .create_untitled("long.txt".into(), "line of content\r\n".repeat(10_000))
        .expect("long document");
    let map = manager.document_map(&long.document_id).expect("long map");
    assert_eq!(map.total_lines, Some(10_001));
    assert!(map.samples.len() <= MAX_DOCUMENT_MAP_SAMPLES + 1);
    assert_eq!(map.samples[1].line, 11);
    assert!(map
        .samples
        .iter()
        .filter(|sample| sample.line <= 10_000)
        .all(|sample| sample.text == "line of content"));
}

#[test]
fn a_large_document_map_samples_whole_lines() {
    let directory = tempfile::tempdir().expect("folder");
    let path = directory.path().join("large.txt");
    let mut text = String::new();
    let mut line = 0;
    while text.len() as u64 <= super::DOCUMENT_MAP_FULL_BYTES {
        use std::fmt::Write as _;
        write!(text, "row {line:07} with some text\r\n").expect("text");
        line += 1;
    }
    // One very long line: its samples fall back to fragments.
    text.push_str(
        &"x".repeat(3 * usize::try_from(super::DOCUMENT_MAP_SAMPLE_BYTES).expect("small")),
    );
    fs::write(&path, &text).expect("write");
    let manager = DocumentManager::default();
    let id = manager.open_file(&path).expect("open").document_id;
    let map = manager.document_map(&id).expect("map");
    assert_eq!(map.total_lines, None);
    assert!(map.samples.len() <= MAX_DOCUMENT_MAP_SAMPLES);
    assert_eq!(map.samples[0].byte_offset, 0);
    let bytes = text.as_bytes();
    for sample in &map.samples {
        let offset = usize::try_from(sample.byte_offset).expect("offset");
        if sample.text.starts_with('x') {
            continue;
        }
        assert!(
            offset == 0 || bytes[offset - 1] == b'\n',
            "{offset} starts a line"
        );
        assert!(
            sample.text.starts_with("row ") && sample.text.ends_with(" with some text"),
            "{:?}",
            sample.text
        );
    }
    assert!(map
        .samples
        .windows(2)
        .all(|pair| pair[0].byte_offset < pair[1].byte_offset));
}

#[test]
fn markdown_preview_reads_unsaved_text_and_rejects_oversized_documents() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("notes.md".into(), "# Draft".into())
        .expect("document");
    manager
        .insert_text(&document.document_id, 0, "**new** ", false)
        .expect("unsaved edit");
    let preview = manager
        .markdown_preview_source(&document.document_id)
        .expect("preview");
    assert_eq!(preview.text, "**new** # Draft");
    assert_eq!(preview.revision, 1);

    let large = manager
        .create_untitled("large.md".into(), String::new())
        .expect("large document");
    manager
        .insert_text(
            &large.document_id,
            0,
            &"x".repeat(usize::try_from(super::MAX_MARKDOWN_PREVIEW_BYTES).expect("test size")),
            false,
        )
        .expect("first insert");
    manager
        .insert_text(&large.document_id, 1, "x", false)
        .expect("second insert");
    assert!(matches!(
        manager.markdown_preview_source(&large.document_id),
        Err(EditorError::MarkdownPreviewTooLarge { .. })
    ));
}

#[test]
fn markdown_preview_images_are_relative_and_bounded() {
    let root = tempfile::tempdir().expect("temporary root");
    let notes = root.path().join("notes");
    fs::create_dir(&notes).expect("notes directory");
    let document_path = notes.join("README.md");
    fs::write(&document_path, "![figure](figure.png)").expect("Markdown file");
    fs::write(notes.join("figure.png"), [1_u8, 2, 3]).expect("image file");
    fs::write(root.path().join("outside.png"), [4_u8]).expect("outside image");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&document_path).expect("open Markdown");
    assert_eq!(
        manager
            .markdown_preview_image(&document.document_id, "figure.png")
            .expect("image"),
        "data:image/png;base64,AQID"
    );
    assert!(matches!(
        manager.markdown_preview_image(&document.document_id, "../outside.png"),
        Err(EditorError::InvalidPreviewImage(_))
    ));
    assert!(matches!(
        manager.markdown_preview_image(&document.document_id, "/etc/passwd"),
        Err(EditorError::InvalidPreviewImage(_))
    ));
}

#[test]
fn untitled_recovery_survives_restart_and_save_rebases_without_losing_undo() {
    let root = tempfile::tempdir().expect("temporary root");
    let manager = DocumentManager::new(root.path().to_path_buf());
    let document = manager
        .create_untitled("draft.md".into(), "start".into())
        .expect("untitled document");
    assert!(document.dirty);
    manager
        .insert_text(&document.document_id, 0, "new ", false)
        .expect("edit");
    manager
        .checkpoint_recovery(&document.document_id)
        .expect("checkpoint");
    drop(manager);

    let reopened = DocumentManager::new(root.path().to_path_buf());
    let candidates = reopened.recovery_candidates().expect("candidates");
    assert_eq!(candidates.len(), 1);
    let restored = reopened
        .restore_recovery(&candidates[0].recovery_id)
        .expect("restore");
    assert!(restored.dirty);
    let text = reopened
        .markdown_preview_source(&restored.document_id)
        .expect("content");
    assert_eq!(text.text, "new start");
    let edited = reopened
        .insert_text(&restored.document_id, restored.revision, "later ", false)
        .expect("edit recovered document");
    let saved_path = root.path().join("saved.md");
    let saved = reopened
        .save_as(&restored.document_id, edited.revision, &saved_path)
        .expect("save recovered document");
    assert!(!saved.dirty);
    assert_eq!(reopened.recovery_candidates().expect("after save"), []);
    reopened
        .undo(&restored.document_id)
        .expect("undo after save");
    let undone = reopened
        .summary(&restored.document_id)
        .expect("dirty summary");
    assert!(undone.dirty);
    reopened
        .checkpoint_recovery(&restored.document_id)
        .expect("checkpoint after save");
    drop(reopened);

    let final_manager = DocumentManager::new(root.path().to_path_buf());
    let candidates = final_manager
        .recovery_candidates()
        .expect("later candidates");
    assert_eq!(candidates.len(), 1);
    let restored = final_manager
        .restore_recovery(&candidates[0].recovery_id)
        .expect("restore after save");
    assert_eq!(
        final_manager
            .markdown_preview_source(&restored.document_id)
            .expect("content")
            .text,
        "new start"
    );
    assert!(restored.dirty);
    final_manager
        .close_document(&restored.document_id, true)
        .expect("discard");
    assert_eq!(
        final_manager.recovery_candidates().expect("no recovery"),
        []
    );
}

#[test]
fn recovery_rejects_a_changed_original_file() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("source.md");
    fs::write(&path, "original").expect("source");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    manager
        .insert_text(&document.document_id, 0, "edited ", false)
        .expect("edit");
    manager
        .checkpoint_recovery(&document.document_id)
        .expect("checkpoint");
    drop(manager);
    fs::write(&path, "changed source").expect("change source");
    let reopened = DocumentManager::new(root.path().join("sessions"));
    let candidates = reopened.recovery_candidates().expect("candidates");
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].source_changed);
    assert!(matches!(
        reopened.restore_recovery(&candidates[0].recovery_id),
        Err(EditorError::Recovery(_))
    ));
    reopened
        .discard_recovery(&candidates[0].recovery_id)
        .expect("discard conflict");
    assert_eq!(reopened.recovery_candidates().expect("after discard"), []);
}

#[test]
fn view_as_marks_the_language_as_chosen_until_save_as_detects_again() {
    let root = tempfile::tempdir().expect("temporary root");
    let manager = DocumentManager::new(root.path().to_path_buf());
    let document = manager
        .create_untitled("notes.txt".into(), "# Title".into())
        .expect("document");
    assert!(!document.language_override);
    manager
        .set_language(&document.document_id, "markdown")
        .expect("set language");
    let edited = manager
        .insert_text(&document.document_id, document.revision, "x", false)
        .expect("edit");
    assert_eq!(edited.language_id, "markdown");
    assert!(edited.language_override);
    assert_eq!(
        serde_json::to_value(&edited).expect("json")["languageOverride"],
        true
    );
    let saved = manager
        .save_as(
            &document.document_id,
            edited.revision,
            &root.path().join("data.json"),
        )
        .expect("save as");
    assert_eq!(saved.language_id, "json");
    assert!(!saved.language_override);
    assert!(serde_json::to_value(&saved)
        .expect("json")
        .get("languageOverride")
        .is_none());
}

#[test]
fn plugin_languages_are_synced_without_breaking_other_plugins_or_documents() {
    let manager = DocumentManager::default();
    let sync = crate::plugin_languages::PluginLanguageSync {
        languages: vec![
            plugin_language("nim", "acme.nim", &["nim"], false),
            // A plugin may add associations to a built-in language.
            plugin_language("json", "acme.json", &["acmejson"], true),
            // But it may not redefine it.
            plugin_language("json", "bad.json", &["broken"], false),
        ],
        errors: Vec::new(),
    };
    let errors = manager.apply_plugin_languages(sync);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("bad.json"));
    assert!(errors[0].contains("built-in language json"));

    let nim = manager
        .create_untitled("script.nim".into(), "echo 1".into())
        .expect("nim document");
    assert_eq!(nim.language_id, "nim");
    let json = manager
        .create_untitled("data.acmejson".into(), "{}".into())
        .expect("json document");
    assert_eq!(json.language_id, "json");

    let conflicts = manager.language_conflicts();
    let json = conflicts
        .iter()
        .find(|conflict| conflict.pattern == "*.json")
        .or_else(|| {
            conflicts
                .iter()
                .find(|conflict| conflict.pattern == "*.acmejson")
        });
    assert!(
        json.is_none(),
        "association-only additions do not claim an existing pattern: {conflicts:?}"
    );

    // A plugin that claims a built-in pattern shows up in the conflict list.
    let claimed = crate::plugin_languages::PluginLanguageSync {
        languages: vec![plugin_language(
            "better-json",
            "zeta.json",
            &["json"],
            false,
        )],
        errors: Vec::new(),
    };
    assert_eq!(manager.apply_plugin_languages(claimed).len(), 0);
    let conflicts = manager.language_conflicts();
    let conflict = conflicts
        .iter()
        .find(|conflict| conflict.pattern == "*.json")
        .expect("json conflict");
    assert_eq!(conflict.claims[0].source, "builtin");
    assert_eq!(conflict.claims[0].language, "json");
    assert_eq!(conflict.claims[1].source, "zeta.json");
    assert_eq!(conflict.claims[1].language, "better-json");

    // Removing every plugin leaves the document readable as Plain Text.
    assert_eq!(
        manager
            .apply_plugin_languages(crate::plugin_languages::PluginLanguageSync::default())
            .len(),
        0
    );
    let refreshed = manager
        .create_untitled("script.nim".into(), "echo 1".into())
        .expect("plain document");
    assert_eq!(refreshed.language_id, "plaintext");
    let view = manager
        .viewport(&ViewportRequest {
            document_id: nim.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(view.lines[0].text, "echo 1");
    assert_eq!(view.syntax.language_id, "plaintext");
}

#[test]
fn a_document_whose_language_is_no_longer_registered_stays_readable() {
    let root = tempfile::tempdir().expect("temporary root");
    let manager = DocumentManager::new(root.path().to_path_buf());
    let document = manager
        .create_untitled("notes.nim".into(), "echo 1".into())
        .expect("document");
    manager
        .checkpoint_recovery(&document.document_id)
        .expect("checkpoint");
    let candidate = manager.recovery_candidates().expect("candidate").remove(0);
    drop(manager);
    // The checkpoint names a language from a plugin that is gone now.
    let manifest = root
        .path()
        .join("recovery")
        .join(format!("{}.json", candidate.recovery_id));
    let mut record: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).expect("manifest")).expect("record");
    record["languageId"] = serde_json::json!("nim");
    fs::write(&manifest, serde_json::to_vec(&record).expect("serialize")).expect("write");

    let reopened = DocumentManager::new(root.path().to_path_buf());
    let restored = reopened
        .restore_recovery(&candidate.recovery_id)
        .expect("restore");
    assert_eq!(restored.language_id, "nim");
    let view = reopened
        .viewport(&ViewportRequest {
            document_id: restored.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(view.lines[0].text, "echo 1");
    assert_eq!(view.syntax.language_id, "plaintext");
    assert_eq!(view.syntax.highlights, Vec::new());
}

#[test]
fn recovery_rejects_an_edit_store_outside_application_data() {
    let root = tempfile::tempdir().expect("temporary root");
    let sessions = root.path().join("sessions");
    let manager = DocumentManager::new(sessions.clone());
    let document = manager
        .create_untitled("draft".into(), "text".into())
        .expect("document");
    manager
        .checkpoint_recovery(&document.document_id)
        .expect("checkpoint");
    let candidate = manager.recovery_candidates().expect("candidate").remove(0);
    let manifest = sessions
        .join("recovery")
        .join(format!("{}.json", candidate.recovery_id));
    let outside = root.path().join("outside.add");
    fs::write(&outside, "outside").expect("outside store");
    let mut record: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).expect("manifest")).expect("record");
    record["additionsPath"] = serde_json::json!(outside);
    fs::write(&manifest, serde_json::to_vec(&record).expect("serialize")).expect("tamper manifest");
    let reopened = DocumentManager::new(sessions);
    assert_eq!(reopened.recovery_candidates().expect("candidate list"), []);
    assert!(matches!(
        reopened.restore_recovery(&candidate.recovery_id),
        Err(EditorError::Recovery(_))
    ));
}

#[test]
fn python_and_markdown_documents_detect_language_from_name() {
    let manager = DocumentManager::default();
    for (name, text, language) in [
        ("script.py", "def greet():\n    return 'hello'\n", "python"),
        ("README.md", "# Heading\nBody\n", "markdown"),
    ] {
        let document = manager
            .create_untitled(name.into(), text.into())
            .expect("document");
        let viewport = manager
            .viewport(&ViewportRequest {
                document_id: document.document_id,
                start_byte: 0,
                start_line: 1,
                line_continuation: false,
                max_bytes: 1024,
            })
            .expect("viewport");
        assert_eq!(viewport.syntax.language_id, language);
        assert_ne!(viewport.syntax.highlights.len(), 0);
    }
}

#[test]
fn transforms_multiple_selections_and_tracks_unicode_expansion() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "straße und groß".into())
        .expect("document");
    manager
        .set_selections(
            &document.document_id,
            vec![
                Selection {
                    anchor_byte: 0,
                    active_byte: 7,
                },
                Selection {
                    anchor_byte: 12,
                    active_byte: 17,
                },
            ],
        )
        .expect("selections");
    manager
        .transform_selections(&document.document_id, 0, TextTransform::Uppercase)
        .expect("uppercase");

    let snapshot = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(snapshot.lines[0].text, "STRASSE und GROSS");
    assert_eq!(snapshot.selections[0].active_byte, 7);
    assert_eq!(snapshot.selections[1].anchor_byte, 12);
    assert_eq!(snapshot.selections[1].active_byte, 17);
}

#[test]
fn bookmarks_relocate_after_edits_and_navigation_wraps() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "abcdef".into())
        .expect("document");
    manager
        .set_selections(&document.document_id, vec![Selection::caret(2)])
        .expect("caret");
    manager
        .toggle_bookmark(&document.document_id)
        .expect("bookmark");
    manager
        .insert_text(&document.document_id, 0, "X", false)
        .expect("insert");
    manager
        .set_selections(&document.document_id, vec![Selection::caret(0)])
        .expect("start caret");
    manager
        .toggle_bookmark(&document.document_id)
        .expect("start bookmark");

    assert_eq!(
        manager
            .move_to_bookmark(&document.document_id, BookmarkDirection::Next)
            .expect("next"),
        vec![Selection::caret(3)]
    );
    assert_eq!(
        manager
            .move_to_bookmark(&document.document_id, BookmarkDirection::Next)
            .expect("wrapped next"),
        vec![Selection::caret(0)]
    );
    let snapshot = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(snapshot.bookmarks, vec![0, 3]);
    assert_eq!(
        serde_json::to_value(snapshot).expect("serialize")["bookmarks"],
        serde_json::json!(["0", "3"])
    );

    manager.undo(&document.document_id).expect("undo insert");
    let undone = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("undo viewport");
    assert_eq!(undone.bookmarks, vec![0, 2]);

    manager.redo(&document.document_id).expect("redo insert");
    let redone = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("redo viewport");
    assert_eq!(redone.bookmarks, vec![0, 3]);
}

#[test]
fn backward_delete_respects_multibyte_utf8_boundaries() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "a中".into())
        .expect("document");
    manager
        .set_selections(&document.document_id, vec![Selection::caret(4)])
        .expect("caret");
    manager
        .delete_backward(&document.document_id, 0)
        .expect("delete");
    let viewport = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(viewport.lines[0].text, "a");
}

#[test]
fn viewport_cursor_does_not_split_a_crlf_ending() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "a\r\nb".into())
        .expect("document");
    let first = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 2,
        })
        .expect("first viewport");
    assert_eq!(first.lines[0].text, "a");
    let cursor = first.next.expect("cursor");
    assert_eq!(cursor.byte_offset, 1);
    let second = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id,
            start_byte: cursor.byte_offset,
            start_line: cursor.line,
            line_continuation: cursor.line_continuation,
            max_bytes: 2,
        })
        .expect("second viewport");
    assert_eq!(second.lines[0].line, 1);
    assert_eq!(second.lines[0].text, "");
    assert_eq!(second.next.expect("next line").line, 2);
}

#[test]
fn viewport_at_reveals_a_deep_offset_without_materializing_a_long_line() {
    let manager = DocumentManager::default();
    let mut text = "short\n".repeat(100);
    text.push_str(&"界".repeat(1_000));
    let document = manager
        .create_untitled("test".into(), text)
        .expect("document");
    let target = 600 + 2_700;
    let viewport = manager
        .viewport_at(&ViewportAtRequest {
            document_id: document.document_id,
            byte_offset: target,
            max_bytes: 128,
        })
        .expect("viewport around offset");

    assert_eq!(viewport.lines[0].line, 101);
    assert!(viewport.lines[0].continuation);
    assert!(viewport.lines[0].start_byte <= target);
    assert!(viewport.next.is_some());
}

#[test]
fn viewport_at_represents_the_empty_final_line_after_an_ending() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "one\n".into())
        .expect("document");
    let viewport = manager
        .viewport_at(&ViewportAtRequest {
            document_id: document.document_id,
            byte_offset: 4,
            max_bytes: 64,
        })
        .expect("final line");
    assert_eq!(viewport.lines.len(), 1);
    assert_eq!(viewport.lines[0].line, 2);
    assert_eq!(viewport.lines[0].start_byte, 4);
    assert_eq!(viewport.lines[0].text, "");
}

#[test]
fn ordinary_viewport_keeps_context_and_empty_final_line_after_enter() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "one\n".into())
        .expect("document");
    manager
        .set_selections(&document.document_id, vec![Selection::caret(4)])
        .expect("final caret");
    let viewport = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 64,
        })
        .expect("viewport");

    assert_eq!(viewport.lines.len(), 2);
    assert_eq!(viewport.lines[0].text, "one");
    assert_eq!(viewport.lines[1].line, 2);
    assert_eq!(viewport.lines[1].start_byte, 4);
    assert_eq!(viewport.lines[1].text, "");
    assert_eq!(viewport.selections, vec![Selection::caret(4)]);
}

#[test]
fn random_line_navigation_uses_sparse_checkpoints_and_survives_edits() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "one\r\ntwo\nthree\rfour".into())
        .expect("document");
    let third = manager
        .viewport_line(&ViewportLineRequest {
            document_id: document.document_id.clone(),
            line: 3,
            max_bytes: 64,
        })
        .expect("third line");
    assert_eq!(third.lines[0].text, "three");

    manager
        .insert_text(&document.document_id, 0, "zero\n", false)
        .expect("insert line");
    let fourth = manager
        .viewport_line(&ViewportLineRequest {
            document_id: document.document_id.clone(),
            line: 4,
            max_bytes: 64,
        })
        .expect("fourth line after edit");
    assert_eq!(fourth.lines[0].text, "three");
    assert!(matches!(
        manager.viewport_line(&ViewportLineRequest {
            document_id: document.document_id,
            line: 99,
            max_bytes: 64,
        }),
        Err(EditorError::LineOutOfRange(99))
    ));
}

#[test]
fn observed_navigation_index_has_a_hard_memory_bound() {
    let mut index = SparseNavigationIndex::default();
    for value in 1..u64::try_from(MAX_NAVIGATION_CHECKPOINTS + 500).expect("fits") {
        index.record(value * 10, value + 1);
    }
    assert_eq!(index.lines_by_offset.len(), MAX_NAVIGATION_CHECKPOINTS);
    assert_eq!(index.lines_by_offset.get(&0), Some(&1));
}

#[test]
fn ipc_offsets_serialize_as_lossless_decimal_strings() {
    let selection = Selection {
        anchor_byte: u64::MAX,
        active_byte: u64::from(u32::MAX) + 1,
    };
    let encoded = serde_json::to_value(selection).expect("serialize");
    assert_eq!(encoded["anchorByte"], u64::MAX.to_string());
    assert_eq!(encoded["activeByte"], "4294967296");
    let decoded: Selection = serde_json::from_value(encoded).expect("deserialize");
    assert_eq!(decoded.anchor_byte, u64::MAX);
}

#[test]
fn opens_a_file_with_a_persistent_disk_add_buffer() {
    let fixture = std::env::temp_dir().join(format!("nulis-core-open-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&fixture);
    fs::create_dir(&fixture).expect("fixture");
    let source = fixture.join("example.txt");
    let session_root = fixture.join("sessions");
    fs::write(&source, b"file-backed\ncontent").expect("source");
    let manager = DocumentManager::new(session_root.clone());

    let document = manager.open_file(&source).expect("open");
    assert_eq!(document.source_path.as_deref(), source.to_str());
    let viewport = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(viewport.lines[0].text, "file-backed");
    assert_eq!(fs::read_dir(session_root).expect("sessions").count(), 1);
    drop(manager);
    fs::remove_dir_all(fixture).expect("cleanup");
}

#[test]
fn saves_untitled_and_existing_documents_atomically() {
    let fixture = std::env::temp_dir().join(format!("nulis-core-save-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&fixture);
    fs::create_dir(&fixture).expect("fixture");
    let destination = fixture.join("saved.txt");
    let manager = DocumentManager::new(fixture.join("sessions"));
    let document = manager
        .create_untitled("Untitled 1".into(), "hello".into())
        .expect("untitled");

    let saved = manager
        .save_as(&document.document_id, document.revision, &destination)
        .expect("save as");
    assert_eq!(saved.name, "saved.txt");
    assert_eq!(saved.source_path.as_deref(), destination.to_str());
    assert_eq!(fs::read(&destination).expect("saved bytes"), b"hello");

    let edited = manager
        .replace(&document.document_id, saved.revision, 0, 5, "updated")
        .expect("edit");
    manager
        .save(&document.document_id, edited.revision)
        .expect("save");
    assert_eq!(fs::read(&destination).expect("updated bytes"), b"updated");

    fs::write(&destination, b"external").expect("external change");
    assert!(matches!(
        manager.save(&document.document_id, edited.revision),
        Err(EditorError::Save(message)) if message.contains("destination changed")
    ));
    assert_eq!(fs::read(&destination).expect("external bytes"), b"external");
    drop(manager);
    fs::remove_dir_all(fixture).expect("cleanup");
}

#[test]
fn windows_1252_override_preserves_source_coordinates_and_encodes_input() {
    let fixture =
        std::env::temp_dir().join(format!("nulis-core-encoding-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&fixture);
    fs::create_dir(&fixture).expect("fixture");
    let source = fixture.join("legacy.txt");
    fs::write(&source, [b'c', b'a', b'f', 0xE9, b'\n']).expect("source");
    let manager = DocumentManager::new(fixture.join("sessions"));
    let document = manager.open_file(&source).expect("open");
    let updated = manager
        .set_encoding(&document.document_id, TextEncoding::Windows1252)
        .expect("encoding");
    assert_eq!(updated.encoding, TextEncoding::Windows1252);

    manager
        .set_selections(&document.document_id, vec![Selection::caret(4)])
        .expect("caret");
    manager
        .insert_text(&document.document_id, 0, "€", false)
        .expect("encoded insert");
    let viewport = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(viewport.encoding, TextEncoding::Windows1252);
    assert_eq!(viewport.lines[0].text, "café€");
    assert_eq!(viewport.lines[0].source_byte_widths, vec![1; 5]);
    assert!(matches!(
        manager.insert_text(&document.document_id, 1, "中", false),
        Err(EditorError::UnrepresentableText { .. })
    ));

    drop(manager);
    fs::remove_dir_all(fixture).expect("cleanup");
}

#[test]
fn horizontal_navigation_uses_unicode_scalar_boundaries() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "a中🙂".into())
        .expect("document");
    assert_eq!(
        manager
            .move_horizontal(
                &document.document_id,
                super::HorizontalDirection::Right,
                false
            )
            .expect("right")[0]
            .active_byte,
        1
    );
    assert_eq!(
        manager
            .move_horizontal(
                &document.document_id,
                super::HorizontalDirection::Right,
                false
            )
            .expect("right")[0]
            .active_byte,
        4
    );
    assert_eq!(
        manager
            .move_horizontal(
                &document.document_id,
                super::HorizontalDirection::Left,
                true
            )
            .expect("shift left")[0],
        Selection {
            anchor_byte: 4,
            active_byte: 1,
        }
    );
}

#[test]
fn vertical_navigation_handles_mixed_endings_and_retains_scalar_column() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "abcd\r\nx\nαβγδ\r12345".into())
        .expect("document");
    manager
        .set_selections(&document.document_id, vec![Selection::caret(3)])
        .expect("caret");

    assert_eq!(
        manager
            .move_vertical(&document.document_id, VerticalDirection::Down, false)
            .expect("down")[0]
            .active_byte,
        7
    );
    assert_eq!(
        manager
            .move_vertical(&document.document_id, VerticalDirection::Down, false)
            .expect("down")[0]
            .active_byte,
        14
    );
    assert_eq!(
        manager
            .move_vertical(&document.document_id, VerticalDirection::Down, true)
            .expect("shift down")[0],
        Selection {
            anchor_byte: 14,
            active_byte: 20,
        }
    );
    assert_eq!(
        manager
            .move_vertical(&document.document_id, VerticalDirection::Up, false)
            .expect("up")[0]
            .active_byte,
        14
    );
}

#[test]
fn scalar_navigation_is_bounded_across_chunks_and_preserves_invalid_bytes() {
    let mut bytes = vec![b'a'; 65_535];
    bytes.extend_from_slice("中".as_bytes());
    let table = PieceTable::from_bytes(bytes);
    assert_eq!(
        scalar_count(&table, 0, table.len(), TextEncoding::Utf8).expect("count"),
        65_536
    );
    assert_eq!(
        byte_at_scalar_column(&table, 0, table.len(), 65_536, TextEncoding::Utf8).expect("column"),
        table.len()
    );

    let malformed = PieceTable::from_bytes(vec![0xff, b'a']);
    assert_eq!(
        scalar_count(&malformed, 0, 2, TextEncoding::Utf8).expect("count"),
        2
    );
    assert_eq!(
        byte_at_scalar_column(&malformed, 0, 2, 1, TextEncoding::Utf8).expect("column"),
        1
    );
}

#[test]
fn overwrite_replaces_scalars_without_consuming_line_endings() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "a中🙂\nnext".into())
        .expect("document");

    let updated = manager
        .insert_text(&document.document_id, 0, "Z", true)
        .expect("overwrite ASCII");
    assert_eq!(updated.revision, 1);
    let updated = manager
        .insert_text(&document.document_id, 1, "Q", true)
        .expect("overwrite multibyte scalar");
    assert_eq!(updated.revision, 2);
    manager
        .set_selections(&document.document_id, vec![Selection::caret(6)])
        .expect("line-end caret");
    let updated = manager
        .insert_text(&document.document_id, 2, "X", true)
        .expect("insert before line ending");
    let viewport = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(updated.revision, 3);
    assert_eq!(viewport.lines[0].text, "ZQ🙂X");
    assert_eq!(viewport.lines[1].text, "next");
}

#[test]
fn clipboard_payload_precedes_revision_checked_cut() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "αbeta".into())
        .expect("document");
    manager
        .set_selections(
            &document.document_id,
            vec![Selection {
                anchor_byte: 2,
                active_byte: 6,
            }],
        )
        .expect("selection");

    let clipboard = manager
        .selection_text(&document.document_id)
        .expect("clipboard text");
    assert_eq!(clipboard.text, "beta");
    assert_eq!(clipboard.revision, 0);
    assert!(!clipboard.had_invalid_utf8);
    let updated = manager
        .delete_selection(&document.document_id, clipboard.revision)
        .expect("cut selection");
    assert_eq!(updated.byte_len, 2);
    assert!(matches!(
        manager.delete_selection(&document.document_id, clipboard.revision),
        Err(EditorError::StaleRevision { .. })
    ));
}

#[test]
fn find_wraps_and_selects_byte_accurate_matches() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "α one ONE one".into())
        .expect("document");
    let options = super::SearchOptions {
        mode: super::SearchMode::Literal,
        case_sensitive: false,
        whole_word: false,
        columns: None,
    };
    let first = manager
        .find(
            &document.document_id,
            "one",
            options,
            0,
            super::FindDirection::Next,
            true,
        )
        .expect("first match");
    assert_eq!(first.selection.expect("selection").anchor_byte, 3);
    let wrapped = manager
        .find(
            &document.document_id,
            "one",
            options,
            document.byte_len,
            super::FindDirection::Next,
            true,
        )
        .expect("wrapped match");
    assert!(wrapped.wrapped);
    assert_eq!(wrapped.selection.expect("selection").anchor_byte, 3);
}

fn first_line(manager: &DocumentManager, document_id: &super::DocumentId) -> String {
    manager
        .viewport(&ViewportRequest {
            document_id: document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport")
        .lines[0]
        .text
        .clone()
}

fn regex_options() -> super::SearchOptions {
    super::SearchOptions {
        mode: super::SearchMode::Regex,
        case_sensitive: true,
        whole_word: false,
        columns: None,
    }
}

#[test]
fn replace_all_fills_in_capture_groups_per_match() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "width=10 height=200".into())
        .expect("document");
    let result = manager
        .replace_all(
            &document.document_id,
            document.revision,
            r"(?<key>\w+)=(\d+)",
            "$2 ${key}",
            regex_options(),
            None,
        )
        .expect("replace all");
    assert_eq!(result.replacements, 2);
    assert_eq!(
        first_line(&manager, &document.document_id),
        "10 width 200 height"
    );
}

#[test]
fn replacing_one_match_fills_in_its_groups_and_checks_it_is_still_a_match() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "a=1 b=2".into())
        .expect("document");
    // "b=2" is bytes 4..7.
    let updated = manager
        .replace_match(
            &document.document_id,
            document.revision,
            r"(\w)=(\d)",
            r"\2:\1",
            regex_options(),
            4,
            7,
        )
        .expect("replace match");
    assert_eq!(first_line(&manager, &document.document_id), "a=1 2:b");
    // A range that is not exactly a match is refused.
    let error = manager
        .replace_match(
            &document.document_id,
            updated.revision,
            r"(\w)=(\d)",
            "x",
            regex_options(),
            1,
            3,
        )
        .expect_err("not a match");
    assert!(error.to_string().contains("changed"), "{error}");
    let error = manager
        .replace_all(
            &document.document_id,
            updated.revision,
            r"(\w)=",
            "$3",
            regex_options(),
            None,
        )
        .expect_err("no group 3");
    assert!(error.to_string().contains("group 3"), "{error}");
}

#[test]
fn replace_all_is_one_undoable_revision() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("test".into(), "one two one".into())
        .expect("document");
    let result = manager
        .replace_all(
            &document.document_id,
            document.revision,
            "one",
            "three",
            super::SearchOptions::default(),
            None,
        )
        .expect("replace all");
    assert_eq!(result.replacements, 2);
    assert_eq!(result.document.revision, 1);
    let viewport = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("viewport");
    assert_eq!(viewport.lines[0].text, "three two three");
    manager.undo(&document.document_id).expect("undo all");
    let restored = manager
        .viewport(&ViewportRequest {
            document_id: document.document_id,
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 1024,
        })
        .expect("restored viewport");
    assert_eq!(restored.lines[0].text, "one two one");
}

#[test]
fn find_in_files_is_recursive_and_bounded() {
    let root = tempfile::tempdir().expect("temp root");
    fs::create_dir(root.path().join("nested")).expect("nested");
    fs::write(root.path().join("a.txt"), "needle once").expect("first file");
    fs::write(root.path().join("nested/b.txt"), "needle twice needle").expect("second file");
    let result = DocumentManager::default()
        .find_in_files(
            root.path(),
            "needle",
            super::SearchOptions::default(),
            &super::FileSearchScope {
                include: Vec::new(),
                recursive: true,
            },
            2,
        )
        .expect("find in files");
    assert_eq!(result.matches.len(), 2);
    assert!(result.truncated);
    assert!(result.files_scanned <= 2);
}

#[test]
fn find_in_files_filters_names_skips_subfolders_and_reports_lines() {
    let root = tempfile::tempdir().expect("temp root");
    fs::create_dir(root.path().join("nested")).expect("nested");
    fs::write(root.path().join("a.RS"), "x\r\n  needle here\n").expect("rust file");
    fs::write(root.path().join("b.md"), "needle").expect("markdown file");
    fs::write(root.path().join("nested/c.rs"), "needle").expect("nested file");
    let result = DocumentManager::default()
        .find_in_files(
            root.path(),
            "needle",
            super::SearchOptions::default(),
            &super::FileSearchScope {
                include: vec!["*.rs".into(), " ".into()],
                recursive: false,
            },
            100,
        )
        .expect("find in files");
    assert_eq!(result.matches.len(), 1);
    let found = &result.matches[0];
    assert!(found.path.ends_with("a.RS"));
    assert_eq!(found.line, 2);
    assert_eq!(found.line_text, "  needle here");
}

#[test]
fn wildcards_match_whole_names() {
    assert!(super::types::wildcard_match("*.rs", "lib.rs"));
    assert!(super::types::wildcard_match("a?c*", "abcdef"));
    assert!(!super::types::wildcard_match("*.rs", "lib.rsx"));
    assert!(super::types::wildcard_match("*", ""));
    assert!(!super::types::wildcard_match("?", ""));
}

fn document_text(manager: &DocumentManager, id: &super::DocumentId) -> String {
    manager.markdown_preview_source(id).expect("text").text
}

fn line_changes(manager: &DocumentManager, id: &super::DocumentId) -> Vec<Option<LineChange>> {
    manager
        .viewport(&ViewportRequest {
            document_id: id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 4096,
        })
        .expect("viewport")
        .lines
        .iter()
        .map(|line| line.change)
        .collect()
}

fn range(start: u64, end: u64) -> Selection {
    Selection {
        anchor_byte: start,
        active_byte: end,
    }
}

#[test]
fn word_selection_uses_one_unicode_rule_and_never_splits_graphemes_or_crlf() {
    let manager = DocumentManager::default();
    let text = "foo_bar(x) caf\u{e9} \u{1f44d}\u{1f3fd}!!\t\tend\r\nnext";
    let document = manager
        .create_untitled("words.py".into(), text.into())
        .expect("document");
    let id = &document.document_id;
    let select = |offset| manager.select_word_at(id, 0, offset).expect("word");
    assert_eq!(select(3), range(0, 7));
    assert_eq!(select(7), range(7, 8));
    assert_eq!(select(12), range(11, 16));
    // The pointer may land between the two scalars of one emoji grapheme.
    assert_eq!(select(21), range(17, 27));
    assert_eq!(select(28), range(27, 29));
    // A line ending selects the adjacent word, even between CR and LF.
    assert_eq!(select(32), range(29, 32));
    assert_eq!(select(33), range(29, 32));
    assert_eq!(select(38), range(34, 38));
    // Double-clicking inside an existing selection replaces it.
    manager.set_selections(id, vec![range(0, 38)]).expect("all");
    assert_eq!(select(35), range(34, 38));
    assert!(matches!(
        manager.select_word_at(id, 9, 0),
        Err(EditorError::StaleRevision { .. })
    ));
    assert!(matches!(
        manager.select_word_at(id, 0, 99),
        Err(EditorError::OffsetOutOfRange { .. })
    ));
}

#[test]
fn word_selection_keeps_malformed_bytes_addressable() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("broken.txt");
    fs::write(&path, b"ab\xffcd").expect("fixture");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    let id = &document.document_id;
    assert_eq!(manager.select_word_at(id, 0, 1).expect("left"), range(0, 2));
    assert_eq!(manager.select_word_at(id, 0, 2).expect("bad"), range(2, 3));
    assert_eq!(
        manager.select_word_at(id, 0, 4).expect("right"),
        range(3, 5)
    );
}

#[test]
fn selected_text_moves_or_copies_exact_bytes_as_one_undo_action() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("move".into(), "one two three\r\nfour".into())
        .expect("document");
    let id = &document.document_id;

    // Forward move: the destination is expressed before removal.
    manager
        .set_selections(id, vec![range(4, 8)])
        .expect("select");
    let moved = manager
        .move_selected_text(id, 0, 4, 8, 13, false)
        .expect("forward move");
    assert_eq!(moved.revision, 1);
    assert_eq!(document_text(&manager, id), "one threetwo \r\nfour");
    manager.undo(id).expect("single undo");
    assert_eq!(document_text(&manager, id), "one two three\r\nfour");
    let redone = manager.redo(id).expect("single redo");
    assert_eq!(document_text(&manager, id), "one threetwo \r\nfour");

    // Backward move of a range that crosses a CRLF ending.
    manager
        .set_selections(id, vec![range(9, 15)])
        .expect("select");
    let moved = manager
        .move_selected_text(id, redone.revision, 9, 15, 0, false)
        .expect("backward move");
    assert_eq!(document_text(&manager, id), "two \r\none threefour");

    // Dropping inside the source range changes nothing.
    manager
        .set_selections(id, vec![range(0, 3)])
        .expect("select");
    let unchanged = manager
        .move_selected_text(id, moved.revision, 0, 3, 2, false)
        .expect("self drop");
    assert_eq!(unchanged.revision, moved.revision);

    // The copy modifier leaves the source in place.
    let copied = manager
        .move_selected_text(id, moved.revision, 0, 3, 4, true)
        .expect("copy");
    assert_eq!(document_text(&manager, id), "two two\r\none threefour");

    // A stale drop is rejected before the source is removed.
    manager
        .set_selections(id, vec![range(0, 3)])
        .expect("select");
    assert!(matches!(
        manager.move_selected_text(id, moved.revision, 0, 3, 9, false),
        Err(EditorError::StaleRevision { .. })
    ));
    // The range must be the authoritative ordinary selection.
    assert!(matches!(
        manager.move_selected_text(id, copied.revision, 1, 3, 9, false),
        Err(EditorError::InvalidRange { .. })
    ));
    assert_eq!(document_text(&manager, id), "two two\r\none threefour");
}

#[test]
fn read_only_mode_refuses_edits_and_character_counts_follow_the_encoding() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("notes.txt");
    // Five characters in seven bytes: two of them take two bytes in UTF-8.
    fs::write(&path, "h\u{e9}ll\u{f8}".as_bytes()).expect("fixture");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    let id = &document.document_id;
    assert!(!document.read_only);
    let count = manager.character_count(id).expect("count");
    assert_eq!(count.characters.as_deref(), Some("5"));
    assert_eq!(document.byte_len, 7);

    let edited = manager.insert_text(id, 0, "!", false).expect("edit");
    let locked = manager.toggle_read_only(id).expect("lock");
    assert!(locked.read_only);
    assert!(locked.dirty);
    let refused = manager
        .insert_text(id, edited.revision, "x", false)
        .expect_err("typing is refused");
    assert!(refused.to_string().contains("read-only"), "{refused}");
    assert!(manager.undo(id).is_err());
    assert!(manager.replace(id, edited.revision, 0, 1, "y").is_err());
    // Nothing changed, and reading and saving still work.
    let unchanged = manager.summary(id).expect("summary");
    assert_eq!(unchanged.revision, edited.revision);
    assert_eq!(unchanged.byte_len, 8);
    assert!(!manager.save(id, edited.revision).expect("save").dirty);

    let unlocked = manager.toggle_read_only(id).expect("unlock");
    assert!(!unlocked.read_only);
    let undone = manager.undo(id).expect("undo works again");
    assert_eq!(undone.byte_len, 7);

    // Windows-1252 has one character per byte.
    manager
        .set_encoding(id, TextEncoding::Windows1252)
        .expect("encoding");
    let count = manager.character_count(id).expect("count");
    assert_eq!(count.characters.as_deref(), Some("7"));
}

#[test]
fn undoing_back_to_the_saved_content_clears_the_dirty_flag() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("notes.txt");
    fs::write(&path, b"one\n").expect("fixture");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    let id = &document.document_id;

    let first = manager.insert_text(id, 0, "a", false).expect("edit");
    let second = manager
        .insert_text(id, first.revision, "b", false)
        .expect("edit");
    assert!(second.dirty);
    assert!(manager.undo(id).expect("undo").dirty);
    let original = manager.undo(id).expect("undo");
    assert!(!original.dirty);
    assert!(!manager.has_dirty_documents().expect("dirty documents"));
    assert_eq!(line_changes(&manager, id), vec![None, None]);
    let redone = manager.redo(id).expect("redo");
    assert!(redone.dirty);

    // After a save the saved content is the clean point in both directions.
    assert!(!manager.save(id, redone.revision).expect("save").dirty);
    assert!(manager.undo(id).expect("undo past save").dirty);
    assert!(!manager.redo(id).expect("redo to save").dirty);
}

#[test]
fn line_change_markers_follow_edits_saves_and_undo() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("notes.txt");
    fs::write(&path, b"one\r\ntwo\rthree\nfour").expect("fixture");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    let id = &document.document_id;
    assert_eq!(line_changes(&manager, id), vec![None; 4]);

    // Multi-caret typing marks every touched logical line and no other.
    manager
        .set_selections(id, vec![Selection::caret(5), Selection::caret(15)])
        .expect("carets");
    let edited = manager.insert_text(id, 0, "*", false).expect("edit");
    let unsaved = Some(LineChange::Unsaved);
    assert_eq!(
        line_changes(&manager, id),
        vec![None, unsaved, None, unsaved]
    );

    // A failed save leaves the markers unsaved.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(&path)
        .and_then(|file| file.set_modified(later))
        .expect("external change");
    assert!(manager.save(id, edited.revision).is_err());
    assert_eq!(
        line_changes(&manager, id),
        vec![None, unsaved, None, unsaved]
    );

    // Save As converts them only after the atomic write succeeds.
    let saved_path = root.path().join("saved.txt");
    let saved = manager
        .save_as(id, edited.revision, &saved_path)
        .expect("save as");
    let kept = Some(LineChange::Saved);
    assert_eq!(line_changes(&manager, id), vec![None, kept, None, kept]);

    // Editing a saved line makes it unsaved; undo restores the saved marker.
    manager
        .set_selections(id, vec![Selection::caret(6)])
        .expect("caret");
    manager
        .insert_text(id, saved.revision, "!", false)
        .expect("edit saved line");
    assert_eq!(line_changes(&manager, id), vec![None, unsaved, None, kept]);
    manager.undo(id).expect("undo");
    assert_eq!(line_changes(&manager, id), vec![None, kept, None, kept]);
    manager.redo(id).expect("redo");
    assert_eq!(line_changes(&manager, id), vec![None, unsaved, None, kept]);

    // Deleting a whole line is shown at the surviving boundary.
    let current = manager.summary(id).expect("summary");
    manager
        .replace(id, current.revision, 0, 5, "")
        .expect("delete first line");
    assert_eq!(line_changes(&manager, id), vec![unsaved, None, kept]);
    let map = manager.document_map(id).expect("map");
    assert!(map.line_changes.available);
    assert_eq!(map.changes.len(), 2);
    assert_eq!(map.changes[0].line, 1);
    // The map counts LF-terminated rows, so the lone CR does not split a row.
    assert_eq!(map.changes[1].line, 2);

    // Undoing an edit made before a save differs from the saved file.
    let before_save = manager.summary(id).expect("summary");
    manager.save(id, before_save.revision).expect("save");
    assert_eq!(line_changes(&manager, id), vec![kept, None, kept]);
    manager.undo(id).expect("undo across save");
    assert_eq!(line_changes(&manager, id)[0], unsaved);
}

#[test]
fn drag_move_marks_source_and_destination_lines() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("move".into(), String::new())
        .expect("document");
    let id = &document.document_id;
    let typed = manager
        .insert_text(id, 0, "one\ntwo\nthree\nfour", false)
        .expect("type");
    let root = tempfile::tempdir().expect("temporary root");
    let saved = manager
        .save_as(id, typed.revision, &root.path().join("moved.txt"))
        .expect("save");
    manager
        .set_selections(id, vec![range(0, 3)])
        .expect("select");
    manager
        .move_selected_text(id, saved.revision, 0, 3, 18, false)
        .expect("move");
    assert_eq!(document_text(&manager, id), "\ntwo\nthree\nfourone");
    assert_eq!(
        line_changes(&manager, id),
        vec![
            Some(LineChange::UnsavedDeletion),
            Some(LineChange::Saved),
            Some(LineChange::Saved),
            Some(LineChange::Unsaved),
        ]
    );
}

#[test]
fn recovered_documents_regain_unsaved_line_markers() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("draft.txt");
    fs::write(&path, b"one\ntwo\nthree\n").expect("fixture");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    manager
        .replace(&document.document_id, 0, 4, 7, "TWO")
        .expect("edit");
    manager
        .checkpoint_recovery(&document.document_id)
        .expect("checkpoint");
    drop(manager);

    let reopened = DocumentManager::new(root.path().join("sessions"));
    let candidates = reopened.recovery_candidates().expect("candidates");
    let restored = reopened
        .restore_recovery(&candidates[0].recovery_id)
        .expect("restore");
    assert_eq!(
        line_changes(&reopened, &restored.document_id),
        vec![None, Some(LineChange::Unsaved), None, None]
    );
}

#[test]
fn line_changes_stay_sparse_for_a_file_beyond_four_gibibytes() {
    // A file extended with `set_len` is sparse on Unix file systems. On
    // Windows it is fully allocated unless explicitly marked sparse, so
    // this test would need gigabytes of free disk and time to zero them.
    // The 64-bit offset arithmetic it exercises is the same everywhere.
    if cfg!(windows) {
        return;
    }
    let root = tempfile::tempdir().expect("temporary root");
    let source_path = root.path().join("huge.txt");
    let deep = 5_u64 * 1024 * 1024 * 1024;
    let mut source = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&source_path)
        .expect("sparse source");
    source.set_len(deep + 64).expect("sparse length");
    source.seek(SeekFrom::Start(deep)).expect("seek");
    source.write_all(b"\nalpha\nbeta\n").expect("lines");
    source.sync_all().expect("sync");

    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&source_path).expect("open sparse source");
    let id = &document.document_id;
    manager
        .replace(id, 0, deep + 1, deep + 6, "ALPHA")
        .expect("deep edit");
    // Read by byte offset: no line counting across the sparse prefix.
    let snapshot = manager
        .viewport(&ViewportRequest {
            document_id: id.clone(),
            start_byte: deep + 1,
            start_line: 2,
            line_continuation: false,
            max_bytes: 4096,
        })
        .expect("deep viewport");
    assert!(snapshot.line_changes.available);
    let marked: Vec<_> = snapshot
        .lines
        .iter()
        .filter(|line| line.change.is_some())
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(marked, vec!["ALPHA"]);
    let map = manager.document_map(id).expect("map");
    assert_eq!(map.changes.len(), 1);
    assert_eq!(map.changes[0].byte_offset, deep + 1);
}

#[test]
fn json_tools_validate_locate_errors_and_format_as_one_undo_step() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("data.json");
    fs::write(&path, "{\"b\":1,\n \"a\":[1,2}\n").expect("fixture");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    let id = &document.document_id;
    assert_eq!(document.language_id, "json");

    let report = manager.validate_document(id).expect("report");
    assert!(!report.valid);
    assert_eq!((report.line, report.column), (Some(2), Some(10)));
    assert_eq!(report.byte_offset.as_deref(), Some("17"));
    assert!(
        report.message.starts_with("Invalid JSON: "),
        "{}",
        report.message
    );
    // An invalid document is never rewritten.
    assert!(matches!(
        manager.format_document(id, document.revision, false, false),
        Err(EditorError::Tool(message)) if message.contains("line 2, column 10")
    ));
    assert_eq!(document_text(&manager, id), "{\"b\":1,\n \"a\":[1,2}\n");

    let fixed = manager
        .replace(id, document.revision, 17, 18, "]}")
        .expect("fix");
    assert!(manager.validate_document(id).expect("report").valid);
    assert!(matches!(
        manager.format_document(id, document.revision, false, false),
        Err(EditorError::StaleRevision { .. })
    ));
    let pretty = manager
        .format_document(id, fixed.revision, false, false)
        .expect("beautify");
    let expected = "{\n  \"b\": 1,\n  \"a\": [\n    1,\n    2\n  ]\n}\n";
    assert_eq!(document_text(&manager, id), expected);
    // Already formatted: no new revision.
    let again = manager
        .format_document(id, pretty.revision, false, false)
        .expect("no-op");
    assert_eq!(again.revision, pretty.revision);
    let minified = manager
        .format_document(id, pretty.revision, true, false)
        .expect("minify");
    assert_eq!(document_text(&manager, id), "{\"b\":1,\"a\":[1,2]}\n");
    manager.undo(id).expect("undo minify");
    assert_eq!(document_text(&manager, id), expected);
    assert!(minified.dirty);
}

#[test]
fn yaml_tools_validate_and_beautify_only_after_accepting_what_is_lost() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("ci.yml");
    fs::write(
        &path,
        "# build job\nname:   build\nsteps:\n    -   run: make\n",
    )
    .expect("fixture");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    let id = &document.document_id;
    assert_eq!(document.capabilities.validate.as_deref(), Some("YAML"));
    assert!(document.capabilities.format_may_lose_data);

    let report = manager.validate_document(id).expect("report");
    assert!(report.valid, "{}", report.message);
    assert_eq!(report.message, "Valid YAML");
    assert!(report
        .beautify_note
        .as_deref()
        .is_some_and(|note| note.contains("removes comments")));
    // Without the user's agreement nothing changes.
    assert!(matches!(
        manager.format_document(id, document.revision, false, false),
        Err(EditorError::Tool(message)) if message.contains("confirm")
    ));
    assert_eq!(
        document_text(&manager, id),
        "# build job\nname:   build\nsteps:\n    -   run: make\n"
    );
    let pretty = manager
        .format_document(id, document.revision, false, true)
        .expect("beautify");
    assert_eq!(
        document_text(&manager, id),
        "name: build\nsteps:\n  - run: make\n"
    );
    manager.undo(id).expect("one undo step");
    assert!(document_text(&manager, id).starts_with("# build job"));
    assert!(pretty.revision > document.revision);

    // A repeated key is located; tags make Beautify unavailable.
    let duplicate = root.path().join("dup.yaml");
    fs::write(&duplicate, "a: 1\nb: 2\na: 3\n").expect("fixture");
    let opened = manager.open_file(&duplicate).expect("open");
    let report = manager
        .validate_document(&opened.document_id)
        .expect("report");
    assert!(!report.valid);
    assert_eq!((report.line, report.column), (Some(3), Some(1)));
    let tagged = root.path().join("stack.yaml");
    fs::write(&tagged, "bucket: !Ref Bucket\n").expect("fixture");
    let opened = manager.open_file(&tagged).expect("open");
    let report = manager
        .validate_document(&opened.document_id)
        .expect("report");
    assert!(report.valid && report.beautify_unavailable);
    assert!(manager
        .format_document(&opened.document_id, opened.revision, false, true)
        .is_err());
}

#[test]
fn a_compiled_applescript_opens_read_only_as_bytes() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("Script.scpt");
    fs::write(&path, b"FasdUAS 1.101.10\x0e\x00\x00").expect("fixture");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    assert_eq!(
        document.binary_format.as_deref(),
        Some("compiled AppleScript")
    );
    assert!(document.read_only);
    // AppleScript source is ordinary text.
    let source = root.path().join("Script.applescript");
    fs::write(&source, "tell application \"Finder\"\nend tell\n").expect("fixture");
    let document = manager.open_file(&source).expect("open");
    assert_eq!(document.binary_format, None);
    assert!(!document.read_only);
}

#[test]
fn a_binary_property_list_opens_read_only_without_text_tools() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("Preferences.plist");
    fs::write(&path, b"bplist00\xd1\x01\x02Q").expect("fixture");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    assert_eq!(
        document.binary_format.as_deref(),
        Some("binary property list")
    );
    assert!(document.read_only);
    assert!(document.capabilities.is_empty());
    // An XML property list is ordinary text.
    let xml = root.path().join("Info.plist");
    fs::write(&xml, "<plist><dict/></plist>\n").expect("fixture");
    let document = manager.open_file(&xml).expect("open");
    assert_eq!(document.binary_format, None);
    assert!(!document.read_only);
}

#[test]
fn plist_tools_check_structure_and_beautify_as_xml() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("Info.plist");
    fs::write(
        &path,
        "<plist version=\"1.0\"><dict><key>A</key><integer>x</integer></dict></plist>\n",
    )
    .expect("fixture");
    let manager = DocumentManager::new(root.path().join("sessions"));
    let document = manager.open_file(&path).expect("open");
    let id = &document.document_id;
    assert_eq!(document.language_id, "plist");
    assert_eq!(document.capabilities.validate.as_deref(), Some("plist"));
    let report = manager.validate_document(id).expect("report");
    assert!(!report.valid);
    assert!(
        report
            .message
            .starts_with("Invalid plist: <integer> must hold"),
        "{}",
        report.message
    );
    assert!(manager
        .format_document(id, document.revision, false, false)
        .is_err());

    fs::write(
        &path,
        "<plist version=\"1.0\"><dict><key>A</key><integer>7</integer></dict></plist>\n",
    )
    .expect("fixed");
    let reopened = DocumentManager::new(root.path().join("sessions2"));
    let document = reopened.open_file(&path).expect("open");
    let id = &document.document_id;
    assert!(reopened.validate_document(id).expect("report").valid);
    reopened
        .format_document(id, document.revision, false, false)
        .expect("beautify");
    assert_eq!(
        document_text(&reopened, id),
        "<plist version=\"1.0\">\n  <dict>\n    <key>A</key>\n    <integer>7</integer>\n  </dict>\n</plist>\n"
    );
    assert!(reopened
        .format_document(id, document.revision + 1, true, false)
        .is_err());
}

#[test]
fn xml_tools_follow_the_document_language_and_encoding() {
    let manager = DocumentManager::default();
    let document = manager
        .create_untitled("feed.xml".into(), "<a><b>caf\u{e9}</b><c></a>".into())
        .expect("document");
    let id = &document.document_id;
    assert_eq!(document.language_id, "xml");
    let report = manager.validate_document(id).expect("report");
    assert!(!report.valid);
    // The column counts characters; the offset counts stored bytes.
    assert_eq!((report.line, report.column), (Some(1), Some(18)));
    assert_eq!(report.byte_offset.as_deref(), Some("18"));
    assert!(report.message.contains("expected </c>"));

    let fixed = manager.replace(id, 0, 15, 18, "<c/>").expect("fix");
    manager
        .format_document(id, fixed.revision, false, false)
        .expect("beautify");
    assert_eq!(
        document_text(&manager, id),
        "<a>\n  <b>caf\u{e9}</b>\n  <c/>\n</a>"
    );
    let current = manager.summary(id).expect("summary");
    assert!(matches!(
        manager.format_document(id, current.revision, true, false),
        Err(EditorError::Tool(message)) if message.contains("JSON only")
    ));

    // Tools are offered by language, not by file name.
    let text = manager
        .create_untitled("notes.txt".into(), "{}".into())
        .expect("text");
    assert!(matches!(
        manager.validate_document(&text.document_id),
        Err(EditorError::Tool(_))
    ));
    manager
        .set_language(&text.document_id, "json")
        .expect("view as json");
    assert!(
        manager
            .validate_document(&text.document_id)
            .expect("report")
            .valid
    );
    assert_eq!(
        manager
            .summary(&text.document_id)
            .expect("summary")
            .language_id,
        "json"
    );
}
