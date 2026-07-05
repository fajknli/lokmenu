// src/main.rs

use clap::Parser;
use std::io::{self, Read};

mod config;
mod keyboard;
mod matcher;
mod pinyin;
mod render;
mod state;
mod wayland;

use config::Config;

#[derive(Parser, Debug)]
#[command(name = "lok", about = "CJK-optimized Wayland menu tool")]
struct Cli {
    /// 提示符前缀
    #[arg(short, long, default_value = "")]
    prompt: String,

    /// 输出选中项的序号而非内容
    #[arg(short, long)]
    output_index: bool,

    /// 垂直显示的行数
    #[arg(short = 'n', long, default_value_t = 8)]
    lines: u32,

    /// 窗口宽度 (像素)
    #[arg(short = 'W', long, default_value_t = 800)]
    width: u32,

    /// 字体大小
    #[arg(short, long, default_value_t = 18.0)]
    font_size: f32,

    /// 字体名称
    #[arg(short = 'f', long, default_value = "Noto Sans CJK SC")]
    font: String,

    /// 背景颜色 (#RRGGBB 或 #RRGGBBAA)
    #[arg(long, default_value = "#111111")]
    bg: String,

    /// 普通文字颜色
    #[arg(long, default_value = "#CCCCCC")]
    fg: String,

    /// 选中项背景颜色
    #[arg(long, default_value = "#333333")]
    sbg: String,

    /// 选中项文字颜色
    #[arg(long, default_value = "#FFFFFF")]
    sfg: String,

    /// 输入框背景颜色
    #[arg(long, default_value = "#1E90FF")]
    prompt_bg: String,

    /// 输入框文字颜色
    #[arg(long, default_value = "#FFFFFF")]
    prompt_fg: String,
}

fn parse_color(s: &str) -> u32 {
    let hex = s.trim_start_matches('#');
    let (r, g, b, a) = if hex.len() == 6 {
        (
            u8::from_str_radix(&hex[0..2], 16).unwrap_or(0x11),
            u8::from_str_radix(&hex[2..4], 16).unwrap_or(0x11),
            u8::from_str_radix(&hex[4..6], 16).unwrap_or(0x11),
            0xFF,
        )
    } else if hex.len() == 8 {
        (
            u8::from_str_radix(&hex[0..2], 16).unwrap_or(0xFF),
            u8::from_str_radix(&hex[2..4], 16).unwrap_or(0xFF),
            u8::from_str_radix(&hex[4..6], 16).unwrap_or(0xFF),
            u8::from_str_radix(&hex[6..8], 16).unwrap_or(0xFF),
        )
    } else {
        (0x11, 0x11, 0x11, 0xFF)
    };
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

fn main() {
    let cli = Cli::parse();

    let config = Config {
        prompt: cli.prompt,
        output_index: cli.output_index,
        lines: cli.lines,
        width: cli.width,
        font_size: cli.font_size,
        font: cli.font, // 传递字体名称
        bg: parse_color(&cli.bg),
        fg: parse_color(&cli.fg),
        sbg: parse_color(&cli.sbg),
        sfg: parse_color(&cli.sfg),
        prompt_bg: parse_color(&cli.prompt_bg),
        prompt_fg: parse_color(&cli.prompt_fg),
    };

    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let items: Vec<String> = input.lines().map(|s| s.to_string()).collect();

    if items.is_empty() {
        std::process::exit(2);
    }

    match wayland::run(items, config) {
        Ok(Some((idx, text))) => {
            if cli.output_index {
                println!("{}", idx);
            } else {
                println!("{}", text);
            }
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
