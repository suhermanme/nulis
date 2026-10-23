use super::{
    expand_snippet, rank, word_prefix, CompletionItem, CompletionKind, CompletionProvider,
    CompletionRequest, KeywordCompletion, SnippetCompletion, WordCompletion, JAVASCRIPT_SNIPPETS,
    PYTHON_SNIPPETS,
};
use nulis_syntax::Vocabulary;

fn request<'a>(before: &'a str, after: &'a str, word_bytes: &'a [u8]) -> CompletionRequest<'a> {
    CompletionRequest {
        prefix: word_prefix(before, word_bytes),
        before,
        after,
        word_bytes,
    }
}

fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|item| item.label.as_str()).collect()
}

fn word(label: &str) -> CompletionItem {
    CompletionItem {
        label: label.into(),
        kind: CompletionKind::Word,
        insert: label.into(),
        snippet: false,
        detail: None,
    }
}

#[test]
fn the_prefix_is_the_word_part_before_the_caret() {
    assert_eq!(word_prefix("let total_cou", b""), "total_cou");
    assert_eq!(word_prefix("x = $sco", b"$"), "$sco");
    assert_eq!(word_prefix("x = $sco", b""), "sco");
    assert_eq!(word_prefix("a.", b""), "");
    assert_eq!(word_prefix("naïve_é", b""), "naïve_é");
}

#[test]
fn words_come_from_the_text_nearest_first_without_the_word_being_typed() {
    let items = WordCompletion.complete(&request(
        "alpha beta gamma 12abc delta x ga",
        "mmaRest omega alpha",
        b"",
    ));
    let found = labels(&items);
    assert_eq!(&found[..3], ["delta", "omega", "gamma"]);
    assert!(!found.contains(&"ga") && !found.contains(&"gammaRest") && !found.contains(&"x"));
    assert!(!found.contains(&"12abc"));
    assert_eq!(found.iter().filter(|label| **label == "alpha").count(), 1);
}

#[test]
fn keywords_follow_the_case_the_user_started_with_in_case_insensitive_languages() {
    let sql = KeywordCompletion {
        vocabulary: Vocabulary {
            keywords: vec!["select"],
            types: vec!["varchar"],
            case_insensitive: true,
            word_bytes: b"",
        },
    };
    let insert = |prefix: &str| {
        sql.complete(&request(prefix, "", b""))
            .into_iter()
            .map(|item| item.insert)
            .collect::<Vec<_>>()
    };
    assert_eq!(insert("SEL"), ["SELECT", "VARCHAR"]);
    assert_eq!(insert("Sel"), ["Select", "Varchar"]);
    assert_eq!(insert("sel"), ["select", "varchar"]);
}

#[test]
fn ranking_prefers_exact_prefixes_then_word_starts_then_letters_in_order() {
    let ranked = rank(
        "gBN",
        vec![vec![
            word("getByName"),
            word("gBNumber"),
            word("globalBufferName"),
            word("other"),
        ]],
    );
    assert_eq!(
        labels(&ranked),
        ["gBNumber", "getByName", "globalBufferName"]
    );
    let ranked = rank(
        "co",
        vec![vec![
            word("Count"),
            word("count"),
            word("cosine"),
            word("decorate"),
        ]],
    );
    assert_eq!(labels(&ranked), ["count", "cosine", "Count", "decorate"]);
    // Snippets and keywords come before words of the same tier; one item per inserted text.
    let keyword = CompletionItem {
        kind: CompletionKind::Keyword,
        ..word("for")
    };
    let snippet = CompletionItem {
        kind: CompletionKind::Snippet,
        insert: "for (${1:x}) {}".into(),
        snippet: true,
        ..word("for")
    };
    let ranked = rank(
        "fo",
        vec![
            vec![snippet],
            vec![keyword],
            vec![word("for"), word("format")],
        ],
    );
    assert_eq!(
        ranked.iter().map(|item| item.kind).collect::<Vec<_>>(),
        [
            CompletionKind::Snippet,
            CompletionKind::Keyword,
            CompletionKind::Word
        ]
    );
    assert_eq!(labels(&ranked), ["for", "for", "format"]);
    // What is already typed is not offered again.
    assert_eq!(rank("count", vec![vec![word("count")]]), Vec::new());
    // An empty prefix (asked for explicitly) offers everything.
    assert_eq!(rank("", vec![vec![word("a"), word("b")]]).len(), 2);
}

#[test]
fn snippets_expand_with_placeholders_lines_and_indentation() {
    let expanded = expand_snippet("if (${1:cond}) {\n\t$0\n}", "\r\n", "  ", "    ");
    assert_eq!(expanded.text, "if (cond) {\r\n      \r\n  }");
    assert_eq!(expanded.stops, [(4, 8), (19, 19)]);
    let expanded = expand_snippet("for (let ${1:i} = 0; $1 < ${2:n}; $1++) {}", "\n", "", "\t");
    assert_eq!(expanded.text, "for (let i = 0; i < n; i++) {}");
    assert_eq!(expanded.stops, [(9, 10), (20, 21), (30, 30)]);
    let expanded = expand_snippet("a \\$1 ${2} b", "\n", "", "\t");
    assert_eq!(expanded.text, "a $1  b");
    assert_eq!(expanded.stops, [(5, 5), (7, 7)]);
    let expanded = expand_snippet("x${0:done}y", "\n", "", "\t");
    assert_eq!(expanded.text, "xdoney");
    assert_eq!(expanded.stops, [(1, 1)]);
}

#[test]
fn built_in_snippets_are_offered_with_their_description() {
    let items = SnippetCompletion {
        snippets: PYTHON_SNIPPETS,
    }
    .complete(&request("de", "", b""));
    let def = items.iter().find(|item| item.label == "def").expect("def");
    assert!(def.snippet);
    assert_eq!(def.detail.as_deref(), Some("Function"));
    for snippet in PYTHON_SNIPPETS.iter().chain(JAVASCRIPT_SNIPPETS) {
        let expanded = expand_snippet(snippet.body, "\n", "", "\t");
        assert!(
            expanded.stops.windows(2).all(|pair| pair[0].0 <= pair[1].0
                || pair[1] == (expanded.text.len(), expanded.text.len())),
            "{}",
            snippet.prefix
        );
        assert!(expanded
            .stops
            .iter()
            .all(|(start, end)| start <= end && *end <= expanded.text.len()));
    }
}

#[test]
fn every_built_in_snippet_expands_cleanly() {
    use super::{
        APPLESCRIPT_SNIPPETS, BASH_SNIPPETS, C_FAMILY_SNIPPETS, GO_SNIPPETS, HTML_SNIPPETS,
        KOTLIN_SNIPPETS, LUA_SNIPPETS, PERL_SNIPPETS, PHP_SNIPPETS, POWERSHELL_SNIPPETS,
        RUBY_SNIPPETS, RUST_SNIPPETS, SQL_SNIPPETS, SWIFT_SNIPPETS, VISUAL_BASIC_SNIPPETS,
        ZIG_SNIPPETS,
    };
    let tables = [
        RUST_SNIPPETS,
        JAVASCRIPT_SNIPPETS,
        PYTHON_SNIPPETS,
        GO_SNIPPETS,
        C_FAMILY_SNIPPETS,
        HTML_SNIPPETS,
        SQL_SNIPPETS,
        BASH_SNIPPETS,
        RUBY_SNIPPETS,
        LUA_SNIPPETS,
        SWIFT_SNIPPETS,
        ZIG_SNIPPETS,
        KOTLIN_SNIPPETS,
        PHP_SNIPPETS,
        POWERSHELL_SNIPPETS,
        PERL_SNIPPETS,
        VISUAL_BASIC_SNIPPETS,
        APPLESCRIPT_SNIPPETS,
    ];
    for snippet in tables.into_iter().flatten() {
        let expanded = expand_snippet(snippet.body, "\n", "", "    ");
        assert!(
            !expanded.text.contains("${"),
            "{}: {}",
            snippet.prefix,
            expanded.text
        );
        assert!(!expanded.stops.is_empty(), "{}", snippet.prefix);
        assert!(expanded
            .stops
            .iter()
            .all(|(start, end)| start <= end && *end <= expanded.text.len()));
    }
    // PHP and PowerShell keep their `$` variables.
    let php = expand_snippet(PHP_SNIPPETS[2].body, "\n", "", "\t");
    assert!(
        php.text.starts_with("foreach ($items as $item)"),
        "{}",
        php.text
    );
}
