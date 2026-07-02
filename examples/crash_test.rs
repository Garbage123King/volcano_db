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

fn main() {
    let addr = "127.0.0.1:15432";
    let mut s = TcpStream::connect(addr).expect("连接 server 失败");

    println!("=== 崩溃恢复测试 ===\n");

    // 1. 提交 Grace (会写入 redo.log)
    println!("[1] BEGIN + INSERT Grace + COMMIT (已提交, redo 将刷盘)");
    send(&mut s, "BEGIN");
    send(&mut s, "INSERT INTO users VALUES (7, 'Grace', 28, 90.0)");
    send(&mut s, "COMMIT");

    // 2. 开启事务插入 Henry 但不提交 (redo 在 buffer 中, 未刷盘)
    println!("[2] BEGIN + INSERT Henry (未提交, redo 在 buffer 中)");
    send(&mut s, "BEGIN");
    send(&mut s, "INSERT INTO users VALUES (8, 'Henry', 35, 77.0)");

    let r = send(&mut s, "SELECT name FROM users");
    println!("[2] 崩溃前查询 (本事务视角): {}", r.lines().last().unwrap());

    // 3. 发送 /crash, server 立即退出 (redo buffer 丢失)
    println!("[3] 发送 /crash, server 将立即退出 (redo buffer 丢失)");
    write_frame(&mut s, "/crash");
    // server 退出后连接断开, read_frame 会失败
    let _ = read_frame(&mut s);

    println!("\n[4] Server 已崩溃. 请重新启动 server 验证 recovery:");
    println!("    .\\target\\debug\\volcano_db.exe server 127.0.0.1:15432");
    println!("    预期: Grace 在 (已提交), Henry 不在 (未提交, buffer 丢失)");
}
