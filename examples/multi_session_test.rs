use std::io::{Read, Write};
use std::net::TcpStream;

fn write_frame(stream: &mut TcpStream, msg: &str) {
    let bytes = msg.as_bytes();
    let len = bytes.len() as u32;
    stream.write_all(&len.to_be_bytes()).unwrap();
    stream.write_all(bytes).unwrap();
    stream.flush().unwrap();
}

fn read_frame(stream: &mut TcpStream) -> String {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).unwrap();
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).unwrap();
    String::from_utf8(buf).unwrap()
}

fn send(stream: &mut TcpStream, sql: &str) -> String {
    write_frame(stream, sql);
    read_frame(stream)
}

fn row_count(resp: &str) -> usize {
    resp.lines()
        .last()
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0)
}

fn main() {
    let addr = "127.0.0.1:15432";
    let mut session_a = TcpStream::connect(addr).expect("连接 server 失败");
    let mut session_b = TcpStream::connect(addr).expect("连接 server 失败");

    println!("=== 多会话读隔离测试 ===\n");

    // 1. 两个会话都看到初始 5 行
    let r = send(&mut session_a, "SELECT name FROM users");
    println!("[A] 初始查询: {} 行", row_count(&r));
    assert_eq!(row_count(&r), 5);

    // 2. Session A 开启事务并插入
    println!("[A] BEGIN");
    send(&mut session_a, "BEGIN");
    println!("[A] INSERT (6, 'Frank', 40, 85.0)");
    send(&mut session_a, "INSERT INTO users VALUES (6, 'Frank', 40, 85.0)");

    // 3. Session A 自己能看到
    let r = send(&mut session_a, "SELECT name FROM users");
    println!("[A] 事务内查询: {} 行 (应=6)", row_count(&r));
    assert_eq!(row_count(&r), 6);

    // 4. Session B 看不到未提交数据 (Read Committed 隔离)
    let r = send(&mut session_b, "SELECT name FROM users");
    println!("[B] 隔离查询: {} 行 (应=5, 看不到未提交)", row_count(&r));
    assert_eq!(row_count(&r), 5);

    // 5. Session A 提交
    println!("[A] COMMIT");
    send(&mut session_a, "COMMIT");

    // 6. Session B 现在能看到了
    let r = send(&mut session_b, "SELECT name FROM users");
    println!("[B] 提交后查询: {} 行 (应=6)", row_count(&r));
    assert_eq!(row_count(&r), 6);

    println!("\n=== 测试通过: 多会话 Read Committed 隔离正常 ===");
}
