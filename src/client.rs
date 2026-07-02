use crate::protocol::{read_frame, write_frame};
use anyhow::Result;
use std::io::{self, Write};
use std::net::TcpStream;

fn print_banner() {
    println!("\x1b[36m");
    println!("  _    __      __                      ____  ____  ");
    println!(" | |  / /___  / /________ _____  ____ / __ \\/ __ ) ");
    println!(" | | / / __ \\/ / ___/ __ `/ __ \\/ __ \\/ / / / __  | ");
    println!(" | |/ / /_/ / / /__/ /_/ / / / / /_/ / /_/ / /_/ /  ");
    println!(" |___/\\____/_/\\___/\\__,_/_/ /_/\\____/_____/_____/   ");
    println!("                                                    ");
    println!("   Volcano DB Client");
    println!("\x1b[0m");
    println!("Type SQL statements followed by a semicolon ';' and press Enter.");
    println!("Type 'exit' or 'quit' to exit.\n");
}

/// 启动 client REPL: 读 SQL -> 发往 server -> 打印响应
pub fn run(addr: &str) -> Result<()> {
    print_banner();
    let mut stream = TcpStream::connect(addr)?;
    println!("\x1b[32m[CLIENT] 已连接到 {}\x1b[0m\n", addr);

    let mut input = String::new();
    loop {
        if input.trim().is_empty() {
            print!("\x1b[32mvolcano_db> \x1b[0m");
        } else {
            print!("\x1b[33m        -> \x1b[0m");
        }
        io::stdout().flush()?;

        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed == "exit" || trimmed == "quit" {
            break;
        }
        // 特殊命令不需要分号结尾, 直接发送
        if trimmed.starts_with('/') {
            write_frame(&mut stream, trimmed)?;
            let resp = read_frame(&mut stream)?;
            println!("{}", resp);
            input.clear();
            continue;
        }
        input.push_str(&line);
        if input.trim().ends_with(';') {
            let sql = input.trim().trim_end_matches(';').trim();
            if !sql.is_empty() {
                write_frame(&mut stream, sql)?;
                let resp = read_frame(&mut stream)?;
                println!("{}", resp);
            }
            input.clear();
        }
    }
    println!("\nGoodbye!");
    Ok(())
}
