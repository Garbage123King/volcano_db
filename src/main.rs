use anyhow::Result;

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  volcano_db server [addr]   启动数据库服务器 (默认 127.0.0.1:5432)");
    eprintln!("  volcano_db client [addr]   启动客户端 (默认 127.0.0.1:5432)");
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("server");
    let addr = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:5432".to_string());

    match mode {
        "server" => volcano_db::server::run(&addr, true),
        "client" => volcano_db::client::run(&addr),
        other => {
            eprintln!("未知模式: {}", other);
            print_usage();
            Ok(())
        }
    }
}
