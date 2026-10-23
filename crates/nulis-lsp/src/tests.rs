use super::{
    file_uri, offset_of, position_of, read_message, write_message, LspError, PositionEncoding,
};
use serde_json::json;
use std::io::BufReader;
use std::path::Path;

#[test]
fn messages_round_trip_through_the_framing() {
    let mut buffer = Vec::new();
    write_message(
        &mut buffer,
        &json!({ "jsonrpc": "2.0", "id": 1, "result": "é" }),
    )
    .expect("write");
    write_message(&mut buffer, &json!({ "jsonrpc": "2.0", "method": "x" })).expect("write");
    let text = String::from_utf8(buffer.clone()).expect("utf-8");
    let (header, rest) = text.split_once("\r\n\r\n").expect("header");
    let first_body = rest.split("Content-Length").next().expect("body");
    assert_eq!(header, format!("Content-Length: {}", first_body.len()));
    let mut reader = BufReader::new(buffer.as_slice());
    assert_eq!(
        read_message(&mut reader).expect("read").expect("one")["result"],
        "é"
    );
    assert_eq!(
        read_message(&mut reader).expect("read").expect("two")["method"],
        "x"
    );
    assert!(read_message(&mut reader).expect("end").is_none());
}

#[test]
fn malformed_and_oversized_messages_are_refused() {
    let mut reader = BufReader::new("Content-Type: x\r\n\r\n{}".as_bytes());
    assert!(matches!(
        read_message(&mut reader),
        Err(LspError::Protocol(_))
    ));
    let mut reader = BufReader::new("Content-Length: 999999999\r\n\r\n".as_bytes());
    assert!(matches!(
        read_message(&mut reader),
        Err(LspError::Protocol(_))
    ));
    let mut reader = BufReader::new("Content-Length: 2\r\n\r\n{x".as_bytes());
    assert!(read_message(&mut reader).is_err());
}

#[test]
fn positions_convert_in_utf8_and_utf16() {
    let text = "a😀b\r\nxé\n";
    // 😀 is 4 bytes and 2 UTF-16 units.
    assert_eq!(position_of(text, 5, PositionEncoding::Utf8), (0, 5));
    assert_eq!(position_of(text, 5, PositionEncoding::Utf16), (0, 3));
    assert_eq!(position_of(text, 8, PositionEncoding::Utf16), (1, 0));
    assert_eq!(position_of(text, 11, PositionEncoding::Utf16), (1, 2));
    assert_eq!(offset_of(text, 0, 3, PositionEncoding::Utf16), 5);
    assert_eq!(offset_of(text, 0, 5, PositionEncoding::Utf8), 5);
    assert_eq!(offset_of(text, 1, 2, PositionEncoding::Utf16), 11);
    // Past the end of a line (and its \r) lands at its end; past the text at the end.
    assert_eq!(offset_of(text, 0, 99, PositionEncoding::Utf16), 6);
    assert_eq!(offset_of(text, 9, 0, PositionEncoding::Utf8), text.len());
    // Inside a multi-byte character rounds down.
    assert_eq!(position_of(text, 3, PositionEncoding::Utf8), (0, 1));
}

#[test]
fn file_uris_escape_what_they_must() {
    assert_eq!(
        file_uri(Path::new("/tmp/a b/é.rs")),
        "file:///tmp/a%20b/%C3%A9.rs"
    );
    assert_eq!(
        file_uri(Path::new("C:\\work\\x.rs")),
        "file:///C:/work/x.rs"
    );
}
