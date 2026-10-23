use crate::editor::indentation::{AutoIndent, IndentSettings, IndentSettingsTable};
use crate::editor::{DocumentId, DocumentManager, EditorError, Selection, ViewportRequest};

fn spaces() -> IndentSettingsTable {
    IndentSettingsTable {
        default: IndentSettings {
            insert_spaces: true,
            tab_size: 4,
            auto_indent: AutoIndent::Full,
            detect: false,
        },
        languages: std::collections::BTreeMap::new(),
    }
}

/// Opens `name` with `text`; each `|` is a caret, `[` … `]` a selection.
fn open(manager: &DocumentManager, name: &str, text: &str) -> (DocumentId, u64) {
    let mut plain = String::new();
    let mut selections = Vec::new();
    let mut anchor = None;
    for character in text.chars() {
        match character {
            '|' => selections.push(Selection::caret(plain.len() as u64)),
            '[' => anchor = Some(plain.len() as u64),
            ']' => selections.push(Selection {
                anchor_byte: anchor.take().expect("open"),
                active_byte: plain.len() as u64,
            }),
            other => plain.push(other),
        }
    }
    let document = manager
        .create_untitled(name.into(), plain)
        .expect("document");
    manager
        .set_selections(&document.document_id, selections)
        .expect("selections");
    (document.document_id, document.revision)
}

/// The text with `|` at carets and `[` … `]` around selections.
fn shown(manager: &DocumentManager, id: &DocumentId) -> String {
    let view = manager
        .viewport(&ViewportRequest {
            document_id: id.clone(),
            start_byte: 0,
            start_line: 1,
            line_continuation: false,
            max_bytes: 64 * 1024,
        })
        .expect("viewport");
    let mut text = manager.markdown_preview_source(id).expect("text").text;
    let mut marks: Vec<(usize, char)> = Vec::new();
    for selection in &view.selections {
        let low = usize::try_from(selection.anchor_byte.min(selection.active_byte)).expect("small");
        let high =
            usize::try_from(selection.anchor_byte.max(selection.active_byte)).expect("small");
        if low == high {
            marks.push((low, '|'));
        } else {
            marks.push((high, ']'));
            marks.push((low, '['));
        }
    }
    marks.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
    for (offset, mark) in marks {
        text.insert(offset, mark);
    }
    text
}

#[test]
fn suggestions_cover_the_typed_word_exactly() {
    let manager = DocumentManager::default();
    let (id, revision) = open(&manager, "a.rs", "let naïveté = 1;\nlet total = naï|");
    let list = manager
        .completions(&id, revision, 7, false, 2)
        .expect("list");
    assert_eq!(list.request_id, 7);
    assert_eq!(list.prefix, "naï");
    assert_eq!(list.replace_end - list.replace_start, "naï".len() as u64);
    assert_eq!(list.items[0].label, "naïveté");
    // Keywords and types of the language are offered too.
    let (id, revision) = open(&manager, "a.rs", "us|");
    let labels: Vec<String> = manager
        .completions(&id, revision, 1, false, 2)
        .expect("list")
        .items
        .into_iter()
        .map(|item| item.label)
        .collect();
    assert!(labels.contains(&"use".to_owned()) && labels.contains(&"usize".to_owned()));
}

#[test]
fn short_words_wait_unless_asked_and_stale_requests_fail() {
    let manager = DocumentManager::default();
    let (id, revision) = open(&manager, "notes.txt", "apple apricot a|");
    assert_eq!(
        manager
            .completions(&id, revision, 1, false, 2)
            .expect("list")
            .items,
        Vec::new()
    );
    let asked = manager
        .completions(&id, revision, 2, true, 2)
        .expect("list");
    assert_eq!(
        asked
            .items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        ["apricot", "apple"]
    );
    assert!(matches!(
        manager.completions(&id, revision + 1, 3, true, 2),
        Err(EditorError::StaleRevision { .. })
    ));
    // A selection gets no suggestions.
    let (id, revision) = open(&manager, "notes.txt", "apple [ap]");
    assert_eq!(
        manager
            .completions(&id, revision, 4, true, 2)
            .expect("list")
            .items,
        Vec::new()
    );
}

#[test]
fn accepting_replaces_the_word_at_every_caret_that_typed_it_in_one_undo_step() {
    let manager = DocumentManager::default();
    let (id, revision) = open(&manager, "a.js", "counter;\nco|\nx co|\nzz|");
    let list = manager
        .completions(&id, revision, 1, false, 2)
        .expect("list");
    let accepted = manager
        .accept_completion(
            &id,
            revision,
            list.replace_start,
            list.replace_end,
            "counter",
            false,
        )
        .expect("accept");
    assert_eq!(
        shown(&manager, &id),
        "counter;\ncounter|\nx counter|\nzzcounter|"
    );
    assert_eq!(accepted.revision, revision + 1);
    manager.undo(&id).expect("undo");
    assert_eq!(
        manager.markdown_preview_source(&id).expect("text").text,
        "counter;\nco\nx co\nzz"
    );
    // A range that no longer ends at the caret is refused.
    let (id, revision) = open(&manager, "a.js", "abc|");
    assert!(manager
        .accept_completion(&id, revision, 0, 2, "x", false)
        .is_err());
}

#[test]
fn a_snippet_selects_its_placeholders_in_turn() {
    let manager = DocumentManager::default();
    manager.set_indent_settings(spaces());
    let (id, revision) = open(&manager, "a.rs", "  fn|");
    let summary = manager
        .accept_completion(&id, revision, 2, 4, "fn ${1:name}($2) {\n\t$0\n}", true)
        .expect("accept");
    assert_eq!(shown(&manager, &id), "  fn [name]() {\n      \n  }");
    // Typing inside a placeholder replaces it; Tab moves on.
    let typed = manager
        .insert_text(&id, summary.revision, "parse", false)
        .expect("type");
    assert_eq!(shown(&manager, &id), "  fn parse|() {\n      \n  }");
    let tabbed = manager.insert_tab(&id, typed.revision).expect("tab");
    assert_eq!(shown(&manager, &id), "  fn parse(|) {\n      \n  }");
    assert_eq!(tabbed.revision, typed.revision);
    manager.insert_tab(&id, tabbed.revision).expect("tab");
    assert_eq!(shown(&manager, &id), "  fn parse() {\n      |\n  }");
    // The last placeholder ends the snippet: Tab indents again.
    let indented = manager.insert_tab(&id, tabbed.revision).expect("tab");
    assert_eq!(shown(&manager, &id), "  fn parse() {\n        |\n  }");
    assert_eq!(indented.revision, tabbed.revision + 1);
}

#[test]
fn leaving_a_placeholder_or_undoing_ends_the_snippet() {
    let manager = DocumentManager::default();
    manager.set_indent_settings(spaces());
    let (id, revision) = open(&manager, "a.rs", "fn|");
    let summary = manager
        .accept_completion(&id, revision, 0, 2, "fn ${1:name}($2) {}", true)
        .expect("accept");
    manager
        .set_selections(&id, vec![Selection::caret(0)])
        .expect("move");
    manager.insert_tab(&id, summary.revision).expect("tab");
    assert_eq!(shown(&manager, &id), "    |fn name() {}");
    let (id, revision) = open(&manager, "b.rs", "fn|");
    manager
        .accept_completion(&id, revision, 0, 2, "fn ${1:name}($2) {}", true)
        .expect("accept");
    let undone = manager.undo(&id).expect("undo");
    manager
        .set_selections(&id, vec![Selection::caret(2)])
        .expect("caret");
    manager.insert_tab(&id, undone.revision).expect("tab");
    assert_eq!(shown(&manager, &id), "fn  |");
}

#[test]
fn completion_opens_by_itself_only_for_languages_with_their_own_words() {
    let manager = DocumentManager::default();
    for (name, mode) in [
        ("a.rs", Some("auto")),
        ("a.html", Some("auto")),
        ("a.txt", Some("manual")),
        ("a.md", Some("manual")),
    ] {
        let document = manager
            .create_untitled(name.into(), String::new())
            .expect("document");
        assert_eq!(document.capabilities.completion.as_deref(), mode, "{name}");
    }
}
