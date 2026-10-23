use super::{
    detect_unit, indent_columns, indent_text, leading_whitespace, Family, IndentContext,
    IndentProvider, IndentUnit, NewlineAction, RequiredUnit, RuleIndenter, APPLESCRIPT, BASH, LUA,
    PLSQL, RUBY, VISUAL_BASIC,
};

const SPACES: IndentUnit = IndentUnit::Spaces(4);

fn indenter(family: Family, comment: Option<&'static str>) -> RuleIndenter {
    RuleIndenter {
        family,
        line_comment: comment,
    }
}

/// `text` holds the lines above and the current line; `|` marks the caret.
fn context_parts(text: &str) -> (Vec<String>, String, String) {
    let mut lines: Vec<&str> = text.split('\n').collect();
    let current = lines.pop().expect("current line");
    let (before, after) = current.split_once('|').expect("caret");
    let previous = lines.iter().rev().map(|line| (*line).to_owned()).collect();
    (previous, before.to_owned(), after.to_owned())
}

fn enter_with(rules: &RuleIndenter, unit: IndentUnit, text: &str) -> NewlineAction {
    let (previous, before, after) = context_parts(text);
    let previous: Vec<&str> = previous.iter().map(String::as_str).collect();
    rules.on_enter(&IndentContext {
        before: &before,
        after: &after,
        previous: &previous,
        unit,
        tab_width: 4,
    })
}

fn enter(rules: &RuleIndenter, text: &str) -> String {
    enter_with(rules, SPACES, text).indent
}

fn typed(rules: &RuleIndenter, text: &str, character: char) -> Option<String> {
    let (previous, before, after) = context_parts(text);
    let previous: Vec<&str> = previous.iter().map(String::as_str).collect();
    rules.on_type(
        &IndentContext {
            before: &before,
            after: &after,
            previous: &previous,
            unit: SPACES,
            tab_width: 4,
        },
        character,
    )
}

#[test]
fn whitespace_helpers_count_tab_stops() {
    assert_eq!(leading_whitespace("\t  x  "), "\t  ");
    assert_eq!(indent_columns("\t  ", 4), 6);
    assert_eq!(indent_columns("  \t", 4), 4);
    assert_eq!(indent_text(6, IndentUnit::Tab, 4), "\t  ");
    assert_eq!(indent_text(6, SPACES, 4), "      ");
    assert_eq!(IndentUnit::Spaces(2).text(), "  ");
    assert_eq!(IndentUnit::Tab.columns(8), 8);
}

#[test]
fn carry_keeps_the_exact_indentation_and_never_adds() {
    let carry = indenter(Family::Carry, None);
    assert_eq!(enter(&carry, "\t  text|"), "\t  ");
    assert_eq!(enter(&carry, "text {|"), "");
    // The caret inside the indentation keeps only what is before it.
    assert_eq!(enter(&carry, "  |  text"), "  ");
}

#[test]
fn braces_open_a_level_and_split_pairs() {
    let c = indenter(Family::Brace, Some("//"));
    assert_eq!(enter(&c, "    if (x) {|"), "        ");
    assert_eq!(enter(&c, "    call(|"), "        ");
    assert_eq!(enter(&c, "    x = 1;|"), "    ");
    // A comment after the brace does not hide it; a brace inside a string is not code.
    assert_eq!(enter(&c, "if (x) { // start|"), "    ");
    assert_eq!(enter(&c, "s = \"{\";|"), "");
    assert_eq!(enter(&c, "s = \"// {\"|"), "");
    let split = enter_with(&c, SPACES, "  fn() {|}");
    assert_eq!(split.indent, "      ");
    assert_eq!(split.closing_indent.as_deref(), Some("  "));
    assert_eq!(
        enter_with(&c, SPACES, "[|]").closing_indent.as_deref(),
        Some("")
    );
    assert_eq!(enter_with(&c, SPACES, "f(a|)").closing_indent, None);
    assert_eq!(
        enter_with(&c, IndentUnit::Tab, "\tif (x) {|").indent,
        "\t\t"
    );
}

#[test]
fn a_closing_brace_moves_to_its_opening_line() {
    let c = indenter(Family::Brace, Some("//"));
    assert_eq!(
        typed(
            &c,
            "  if (x) {\n      a();\n      if (y) { b(); }\n      |",
            '}'
        )
        .as_deref(),
        Some("  ")
    );
    assert_eq!(
        typed(&c, "items = [\n        1,\n        |", ']').as_deref(),
        Some("")
    );
    // Already in place, or not alone on the line: nothing changes.
    assert_eq!(typed(&c, "if (x) {\n    a();\n|", '}'), None);
    assert_eq!(typed(&c, "if (x) {\n    a(|", ')'), None);
    // No opening bracket in sight: one level left.
    assert_eq!(typed(&c, "        |", '}').as_deref(), Some("    "));
}

#[test]
fn python_blocks_follow_colons_and_flow_keywords() {
    let py = indenter(Family::Python, Some("#"));
    assert_eq!(enter(&py, "def f(x):|"), "    ");
    assert_eq!(enter(&py, "    if x:  # check|"), "        ");
    assert_eq!(enter(&py, "        return x|"), "    ");
    assert_eq!(enter(&py, "    pass|"), "");
    assert_eq!(enter(&py, "    values = [|"), "        ");
    assert_eq!(enter(&py, "    s = \"a:\"|"), "    ");
    assert_eq!(
        typed(&py, "if x:\n    a = 1\n    else|", ':').as_deref(),
        Some("")
    );
    assert_eq!(
        typed(
            &py,
            "try:\n    if y:\n        a()\n    except ValueError|",
            ':'
        )
        .as_deref(),
        Some("")
    );
    assert_eq!(typed(&py, "if x:\n    a = 1\nelse|", ':'), None);
    assert_eq!(typed(&py, "    value|", ':'), None);
}

#[test]
fn yaml_opens_for_keys_without_values_and_block_text_and_needs_spaces() {
    let yaml = indenter(Family::Yaml, Some("#"));
    assert_eq!(yaml.required_unit(), Some(RequiredUnit::Spaces));
    assert_eq!(
        enter_with(&yaml, IndentUnit::Spaces(2), "jobs:|").indent,
        "  "
    );
    assert_eq!(
        enter_with(&yaml, IndentUnit::Spaces(2), "  - name:|").indent,
        "    "
    );
    assert_eq!(
        enter_with(&yaml, IndentUnit::Spaces(2), "  run: |-|").indent,
        "    "
    );
    assert_eq!(
        enter_with(&yaml, IndentUnit::Spaces(2), "  key: value  # note|").indent,
        "  "
    );
    assert_eq!(
        enter_with(&yaml, IndentUnit::Spaces(2), "url: \"http://x\"|").indent,
        ""
    );
}

#[test]
fn makefile_recipes_get_a_tab() {
    let make = indenter(Family::Makefile, Some("#"));
    assert_eq!(make.required_unit(), Some(RequiredUnit::Tabs));
    assert_eq!(enter(&make, "build: main.o util.o|"), "\t");
    assert_eq!(enter(&make, "\t$(CC) -o $@ $^|"), "\t");
    assert_eq!(enter(&make, "CFLAGS := -O2|"), "");
    assert_eq!(enter(&make, "FILES = a:b|"), "");
    assert_eq!(enter(&make, "# build: notes|"), "");
}

#[test]
fn markup_elements_open_levels_and_closing_tags_move_left() {
    let html = indenter(Family::Markup, None);
    assert_eq!(enter(&html, "  <div class=\"a\">|"), "      ");
    assert_eq!(enter(&html, "  <br>|"), "  ");
    assert_eq!(enter(&html, "  <img src=\"x\"/>|"), "  ");
    assert_eq!(enter(&html, "  <p>text</p>|"), "  ");
    assert_eq!(enter(&html, "<?xml version=\"1.0\"?>|"), "");
    assert_eq!(enter(&html, "<!-- note -->|"), "");
    let split = enter_with(&html, SPACES, "<ul>|</ul>");
    assert_eq!(split.indent, "    ");
    assert_eq!(split.closing_indent.as_deref(), Some(""));
    assert_eq!(
        typed(&html, "<ul>\n    <li>a</li>\n    <|", '/').as_deref(),
        Some("")
    );
    // An empty element closes level with its opening tag.
    assert_eq!(typed(&html, "<ul>\n    <|", '/').as_deref(), Some(""));
    // Already in place: nothing changes.
    assert_eq!(typed(&html, "<ul>\n    <li>a</li>\n<|", '/'), None);
}

#[test]
fn ruby_and_lua_blocks_open_and_close_with_keywords() {
    let ruby = indenter(Family::Keywords(&RUBY), Some("#"));
    assert_eq!(enter(&ruby, "def greet(name)|"), "    ");
    assert_eq!(enter(&ruby, "  items.each do |item||"), "      ");
    assert_eq!(enter(&ruby, "  return 1 if x|"), "  ");
    assert_eq!(enter(&ruby, "  def x; end|"), "  ");
    // `end` alone on its line moves left on Enter, then the new line follows it.
    let end = enter_with(&ruby, SPACES, "def a\n    b\n    end|");
    assert_eq!(end.current_indent.as_deref(), Some(""));
    assert_eq!(end.indent, "");
    let empty = enter_with(&ruby, SPACES, "def a\n    end|");
    assert_eq!(empty.current_indent.as_deref(), Some(""));
    let else_line = enter_with(&ruby, SPACES, "if a\n    b\n    else|");
    assert_eq!(else_line.current_indent.as_deref(), Some(""));
    assert_eq!(else_line.indent, "    ");
    // Typing `endpoint` is not a closer.
    assert_eq!(
        enter_with(&ruby, SPACES, "def a\n    endpoint|").current_indent,
        None
    );

    let lua = indenter(Family::Keywords(&LUA), Some("--"));
    assert_eq!(enter(&lua, "local function f()|"), "    ");
    assert_eq!(enter(&lua, "if x then|"), "    ");
    assert_eq!(enter(&lua, "for i = 1, 3 do|"), "    ");
    assert_eq!(enter(&lua, "f = function() end|"), "");
}

#[test]
fn shell_visual_basic_plsql_and_applescript_blocks() {
    let bash = indenter(Family::Keywords(&BASH), Some("#"));
    assert_eq!(enter(&bash, "if [ -f x ]; then|"), "    ");
    assert_eq!(enter(&bash, "for f in *.txt; do|"), "    ");
    assert_eq!(enter(&bash, "main() {|"), "    ");
    assert_eq!(
        enter_with(&bash, SPACES, "if x; then\n    a\n    fi|")
            .current_indent
            .as_deref(),
        Some("")
    );

    let vb = indenter(Family::Keywords(&VISUAL_BASIC), Some("'"));
    assert_eq!(enter(&vb, "Public Sub Main()|"), "    ");
    assert_eq!(enter(&vb, "    If x > 1 Then|"), "        ");
    assert_eq!(enter(&vb, "    If x Then y = 1|"), "    ");
    assert_eq!(enter(&vb, "Select Case x|"), "    ");
    let end_sub = enter_with(&vb, SPACES, "Sub Main()\n    Call A()\n    End Sub|");
    assert_eq!(end_sub.current_indent.as_deref(), Some(""));

    let plsql = indenter(Family::Keywords(&PLSQL), Some("--"));
    assert_eq!(enter(&plsql, "CREATE OR REPLACE PROCEDURE p IS|"), "    ");
    assert_eq!(enter(&plsql, "  LOOP|"), "      ");
    assert_eq!(
        enter_with(&plsql, SPACES, "BEGIN\n    NULL;\n    END;|")
            .current_indent
            .as_deref(),
        Some("")
    );

    let applescript = indenter(Family::Keywords(&APPLESCRIPT), Some("--"));
    assert_eq!(enter(&applescript, "tell application \"Finder\"|"), "    ");
    assert_eq!(
        enter(&applescript, "tell application \"Finder\" to activate|"),
        ""
    );
    assert_eq!(enter(&applescript, "on run argv|"), "    ");
}

#[test]
fn indentation_is_detected_from_the_text() {
    assert_eq!(detect_unit("a\n\tb\n\tc\n\t\td\n"), Some(IndentUnit::Tab));
    assert_eq!(
        detect_unit("a:\n  b:\n    c: 1\n  d: 2\n  e:\n    f: 3\n"),
        Some(IndentUnit::Spaces(2))
    );
    assert_eq!(
        detect_unit("fn a() {\n    if x {\n        y();\n    }\n    z(\n      1);\n}\n"),
        Some(IndentUnit::Spaces(4))
    );
    assert_eq!(detect_unit("one\ntwo\n  three\n"), None);
    assert_eq!(detect_unit(""), None);
}
