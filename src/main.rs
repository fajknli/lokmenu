use clap::Parser;
use std::io::{self, Read};

mod config;
mod keyboard;
mod matcher;
mod pinyin;
mod render;
mod state;
mod wayland;

use std::io::IsTerminal;
use config::{Config, WindowAnchor};



#[derive(Parser, Debug)]
#[command(name = "lok", about = "CJK-optimized Wayland menu tool")]
struct Cli {
    /// 提示符前缀
    #[arg(short = 'p', long, default_value = "")]
    prompt: String,

    /// 输出选中项的序号而非内容
    #[arg(short = 'i', long)]
    output_index: bool,

    /// 垂直显示的行数 (0 表示按条目数自适应，最大20)
    #[arg(short = 'n', long, default_value_t = 0)]
    lines: u32,

    /// 启用多选模式 (按 Tab 标记/取消标记，回车返回所有标记项)
    #[arg(short = 'm', long)]
    multi_select: bool,

    /// 多选时输出用 NUL (\0) 分隔，方便配合 xargs -0 使用
    #[arg(short = '0', long)]
    null: bool,

    /// 密码输入模式 (隐藏输入内容，不显示列表)
    #[arg(short = 'P', long)]
    password: bool,

    /// 窗口位置: top, top-left, top-center, top-right, bottom, bottom-left, bottom-center, bottom-right
    #[arg(long, value_enum, default_value_t = WindowAnchor::Top)]
    anchor: WindowAnchor,

    /// 窗口宽度 (0 表示铺满屏幕宽度)
    #[arg(short = 'W', long, default_value_t = 0)]
    width: u32,

    /// 字体大小
    #[arg(short = 's', long, default_value_t = 14.0)]
    font_size: f32,

    /// 字体名称 (留空则使用系统默认字体)
    #[arg(short = 'f', long, default_value = "")]
    font: String,

    /// 背景颜色
    #[arg(short = 'b', long, default_value = "#141522")]
    bg: String,

    /// 普通文字颜色
    #[arg(long, default_value = "#D4DCF2")]
    fg: String,

    /// 选中项背景颜色
    #[arg(long, default_value = "#242838")]
    sbg: String,

    /// 选中项文字颜色
    #[arg(long, default_value = "#D4DCF2")]
    sfg: String,

    /// 匹配字符高亮颜色
    #[arg(long, default_value = "#D70000")]
    hfg: String,

    /// 输入框文字颜色
    #[arg(long, default_value = "#D4DCF2")]
    prompt_fg: String,

    /// 输入框背景颜色
    #[arg(long, default_value = "#1B1D2B")]
    prompt_bg: String,

    /// 提示符前缀文字颜色
    #[arg(long, default_value = "#D4DCF2")]
    prefix_fg: String,

    /// 提示符前缀背景颜色
    #[arg(long, default_value = "#1B1D2B")]
    prefix_bg: String,

    /// 禁用历史记录功能 (不读取/不写入缓存文件)
    #[arg(long = "no-history", default_value_t = false)]
    no_history: bool,
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
    // 恢复 Unix 默认的 SIGPIPE 行为，防止管道破裂时 panic
    #[cfg(target_family = "unix")]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let cli = Cli::parse();

    let mut input = String::new();
    // 如果 stdin 不是终端（说明有管道数据传过来），则读取数据
    // 如果是终端（直接运行程序），则跳过读取，避免阻塞卡死
    if !io::stdin().is_terminal() {
        io::stdin().read_to_string(&mut input).unwrap();
    }

    // 鲁棒性修复：限制最大读取条目数，防止 OOM
    const MAX_ITEMS: usize = 100_000;
    // 把制表符替换成 4 个空格，防止字体不支持导致显示方块
    let items: Vec<String> = input.lines()
        .take(MAX_ITEMS)
        .map(|s| s.replace('\t', "    "))
        .collect();

    let lines = if cli.lines == 0 {
        items.len().min(20) as u32
    } else {
        cli.lines
    };

    let config = Config {
        prompt: cli.prompt,
        output_index: cli.output_index,
        lines,
        width: cli.width,
        font_size: cli.font_size,
        font: cli.font,
        bg: parse_color(&cli.bg),
        fg: parse_color(&cli.fg),
        sbg: parse_color(&cli.sbg),
        sfg: parse_color(&cli.sfg),
        hfg: parse_color(&cli.hfg),
        prompt_fg: parse_color(&cli.prompt_fg),
        prompt_bg: parse_color(&cli.prompt_bg),
        prefix_fg: parse_color(&cli.prefix_fg),
        prefix_bg: parse_color(&cli.prefix_bg),
        multi_select: cli.multi_select,
        null: cli.null,
        password: cli.password,
        anchor: cli.anchor,
        no_history: cli.no_history,
    };

    match wayland::run(items, config) {
        Ok(Some((indices, text))) => {
            if cli.output_index {
                if !indices.is_empty() {
                    let sep = if cli.null { "\0" } else { "\n" };
                    let out: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
                    print!("{}", out.join(sep));
                    if !cli.null { println!(); }
                }
            } else {
                print!("{}", text);
                if !text.ends_with('\n') && !text.ends_with('\0') {
                    println!();
                }
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
