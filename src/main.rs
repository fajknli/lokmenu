use clap::Parser;
use std::io::{self, Read};

mod matcher;
mod pinyin;
mod wayland;

#[derive(Parser, Debug)]
#[command(name = "lok", about = "CJK-optimized Wayland menu tool")]
struct Cli {
    #[arg(short, long, default_value = "")]
    prompt: String,

    #[arg(short, long)]
    output_index: bool,
}

fn main() {
    let _cli = Cli::parse();

    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let items: Vec<String> = input.lines().map(|s| s.to_string()).collect();

    if items.is_empty() {
        std::process::exit(2);
    }

    match wayland::run(items) {
        Ok(Some(selected)) => {
            println!("{}", selected);
            std::process::exit(0);
        }
        Ok(None) => {
            std::process::exit(1); // 用户取消
        }
        Err(e) => {
            eprintln!("lok: 无法连接 Wayland 显示服务器。 {:?}", e);
            std::process::exit(3);
        }
    }
}
