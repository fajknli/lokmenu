// src/keyboard.rs

/// 将 Linux evdev 扫描码转换为 US 布局字符
/// 返回 None 表示是不可打印的控制键
pub fn get_char(key: u32, shift: bool) -> Option<char> {
    match key {
        // 数字行
        2  => Some(if shift { '!' } else { '1' }),
        3  => Some(if shift { '@' } else { '2' }),
        4  => Some(if shift { '#' } else { '3' }),
        5  => Some(if shift { '$' } else { '4' }),
        6  => Some(if shift { '%' } else { '5' }),
        7  => Some(if shift { '^' } else { '6' }),
        8  => Some(if shift { '&' } else { '7' }),
        9  => Some(if shift { '*' } else { '8' }),
        10 => Some(if shift { '(' } else { '9' }),
        11 => Some(if shift { ')' } else { '0' }),
        12 => Some(if shift { '_' } else { '-' }),
        13 => Some(if shift { '+' } else { '=' }),

        // 第一排字母
        16 => Some(if shift { 'Q' } else { 'q' }),
        17 => Some(if shift { 'W' } else { 'w' }),
        18 => Some(if shift { 'E' } else { 'e' }),
        19 => Some(if shift { 'R' } else { 'r' }),
        20 => Some(if shift { 'T' } else { 't' }),
        21 => Some(if shift { 'Y' } else { 'y' }),
        22 => Some(if shift { 'U' } else { 'u' }),
        23 => Some(if shift { 'I' } else { 'i' }),
        24 => Some(if shift { 'O' } else { 'o' }),
        25 => Some(if shift { 'P' } else { 'p' }),
        26 => Some(if shift { '{' } else { '[' }),
        27 => Some(if shift { '}' } else { ']' }),

        // 第二排字母
        30 => Some(if shift { 'A' } else { 'a' }),
        31 => Some(if shift { 'S' } else { 's' }),
        32 => Some(if shift { 'D' } else { 'd' }),
        33 => Some(if shift { 'F' } else { 'f' }),
        34 => Some(if shift { 'G' } else { 'g' }),
        35 => Some(if shift { 'H' } else { 'h' }),
        36 => Some(if shift { 'J' } else { 'j' }),
        37 => Some(if shift { 'K' } else { 'k' }),
        38 => Some(if shift { 'L' } else { 'l' }),
        39 => Some(if shift { ':' } else { ';' }),
        40 => Some(if shift { '"' } else { '\'' }),
        41 => Some(if shift { '~' } else { '`' }),
        43 => Some(if shift { '|' } else { '\\' }),

        // 第三排字母
        44 => Some(if shift { 'Z' } else { 'z' }),
        45 => Some(if shift { 'X' } else { 'x' }),
        46 => Some(if shift { 'C' } else { 'c' }),
        47 => Some(if shift { 'V' } else { 'v' }),
        48 => Some(if shift { 'B' } else { 'b' }),
        49 => Some(if shift { 'N' } else { 'n' }),
        50 => Some(if shift { 'M' } else { 'm' }),
        51 => Some(if shift { '<' } else { ',' }),
        52 => Some(if shift { '>' } else { '.' }),
        53 => Some(if shift { '?' } else { '/' }),

        // 空格
        57 => Some(' '),

        // 其余所有键 (包括 Shift, Ctrl, Alt, Tab, Esc 等) 均不返回可打印字符
        _ => None,
    }
}
