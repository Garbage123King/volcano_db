use std::io::{Cursor, ErrorKind};
use volcano_db::protocol::{read_frame, write_frame};

#[test]
fn test_write_then_read_roundtrip() {
    let mut buf: Vec<u8> = Vec::new();
    let payload = "SELECT * FROM users";
    write_frame(&mut buf, payload).unwrap();
    assert!(!buf.is_empty());

    let mut cursor = Cursor::new(buf);
    let result = read_frame(&mut cursor).unwrap();
    assert_eq!(result, payload);
}

#[test]
fn test_empty_payload() {
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, "").unwrap();
    // 4 bytes length prefix only
    assert_eq!(buf.len(), 4);

    let mut cursor = Cursor::new(buf);
    let result = read_frame(&mut cursor).unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_unicode_payload() {
    let mut buf: Vec<u8> = Vec::new();
    let payload = "INSERT INTO users VALUES (1, '张三', 25, 95.5); -- 中文注释";
    write_frame(&mut buf, payload).unwrap();

    let mut cursor = Cursor::new(buf);
    let result = read_frame(&mut cursor).unwrap();
    assert_eq!(result, payload);
}

#[test]
fn test_multiple_frames_in_sequence() {
    let mut buf: Vec<u8> = Vec::new();
    write_frame(&mut buf, "BEGIN").unwrap();
    write_frame(&mut buf, "INSERT INTO t VALUES (1)").unwrap();
    write_frame(&mut buf, "COMMIT").unwrap();

    let mut cursor = Cursor::new(buf);
    assert_eq!(read_frame(&mut cursor).unwrap(), "BEGIN");
    assert_eq!(read_frame(&mut cursor).unwrap(), "INSERT INTO t VALUES (1)");
    assert_eq!(read_frame(&mut cursor).unwrap(), "COMMIT");
}

#[test]
fn test_large_payload() {
    let mut buf: Vec<u8> = Vec::new();
    // 10KB payload
    let payload = "A".repeat(10_000);
    write_frame(&mut buf, &payload).unwrap();

    let mut cursor = Cursor::new(buf);
    let result = read_frame(&mut cursor).unwrap();
    assert_eq!(result.len(), 10_000);
    assert_eq!(result, payload);
}

#[test]
fn test_read_frame_eof_on_empty_stream() {
    let buf: Vec<u8> = Vec::new();
    let mut cursor = Cursor::new(buf);
    let result = read_frame(&mut cursor);
    assert!(result.is_err());
    let err = result.unwrap_err();
    // Should be UnexpectedEof or similar I/O error
    assert!(
        err.to_string().contains("EOF")
            || err.to_string().contains("eof")
            || err.to_string().contains("unexpected")
            || err.downcast_ref::<std::io::Error>()
                .map(|e| e.kind() == ErrorKind::UnexpectedEof)
                .unwrap_or(false)
    );
}

#[test]
fn test_read_frame_eof_after_partial_length() {
    // Only 2 of 4 length bytes
    let buf = vec![0u8, 0];
    let mut cursor = Cursor::new(buf);
    let result = read_frame(&mut cursor);
    assert!(result.is_err());
}

#[test]
fn test_read_frame_eof_after_length_but_no_payload() {
    // 4 bytes length = 100, but no payload follows
    let mut len_bytes = (100u32).to_be_bytes().to_vec();
    len_bytes.resize(4, 0);
    let buf = (100u32).to_be_bytes().to_vec();
    let mut cursor = Cursor::new(buf);
    let result = read_frame(&mut cursor);
    assert!(result.is_err());
}

#[test]
fn test_read_frame_eof_after_partial_payload() {
    // Length says 10 bytes, but only 5 follow
    let mut buf = (10u32).to_be_bytes().to_vec();
    buf.extend_from_slice(b"hello"); // only 5 of 10
    let mut cursor = Cursor::new(buf);
    let result = read_frame(&mut cursor);
    assert!(result.is_err());
}

#[test]
fn test_write_frame_returns_ok() {
    let mut buf: Vec<u8> = Vec::new();
    let result = write_frame(&mut buf, "test");
    assert!(result.is_ok());
}

#[test]
fn test_read_frame_invalid_utf8() {
    // 4 bytes length = 4, then 4 invalid UTF-8 bytes
    let mut buf = (4u32).to_be_bytes().to_vec();
    buf.extend_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC]);
    let mut cursor = Cursor::new(buf);
    let result = read_frame(&mut cursor);
    assert!(result.is_err());
}
