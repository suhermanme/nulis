//! Every indentation rule family on arbitrary text: no panics, and the
//! indentation it returns is whitespace only.
#![no_main]

use libfuzzer_sys::fuzz_target;
use nulis_language::indent::{
    Family, IndentContext, IndentProvider, IndentUnit, RuleIndenter, APPLESCRIPT, BASH, LUA,
    PLSQL, RUBY, VISUAL_BASIC,
};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let mut lines: Vec<&str> = text.split('\n').collect();
    let current = lines.pop().unwrap_or_default();
    let split = current
        .char_indices()
        .nth(current.chars().count() / 2)
        .map_or(current.len(), |(index, _)| index);
    let (before, after) = current.split_at(split);
    let previous: Vec<&str> = lines.into_iter().rev().take(64).collect();
    let families = [
        Family::Carry,
        Family::Brace,
        Family::Python,
        Family::Yaml,
        Family::Makefile,
        Family::Markup,
        Family::Keywords(&RUBY),
        Family::Keywords(&LUA),
        Family::Keywords(&BASH),
        Family::Keywords(&VISUAL_BASIC),
        Family::Keywords(&PLSQL),
        Family::Keywords(&APPLESCRIPT),
    ];
    for family in families {
        for comment in [None, Some("//"), Some("#"), Some("--")] {
            for unit in [IndentUnit::Tab, IndentUnit::Spaces(2)] {
                let rules = RuleIndenter { family, line_comment: comment };
                let context = IndentContext { before, after, previous: &previous, unit, tab_width: 4 };
                let action = rules.on_enter(&context);
                let whitespace = |text: &str| text.chars().all(|c| c == ' ' || c == '\t');
                assert!(whitespace(&action.indent));
                assert!(action.closing_indent.as_deref().map_or(true, whitespace));
                assert!(action.current_indent.as_deref().map_or(true, whitespace));
                for typed in ['}', ')', ']', ':', '/', 'd', 'é'] {
                    assert!(rules.on_type(&context, typed).as_deref().map_or(true, whitespace));
                }
            }
        }
    }
});
