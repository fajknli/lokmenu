// src/matcher.rs

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use crate::pinyin::PinyinData;

const MAX_MATCHES: usize = 500;

pub struct MatchResult {
    pub original_idx: usize,
    pub score: i32,
    pub highlight_indices: HashSet<usize>,
}

// 线程局部内存池：避免每次匹配时的 Vec 堆分配
thread_local! {
    static FUZZY_BUF: RefCell<FuzzyBuffers> = RefCell::new(FuzzyBuffers::new());
}

struct FuzzyBuffers {
    tchars: Vec<char>,
    qchars: Vec<char>,
    dp_prev: Vec<i32>,
    dp_cur: Vec<i32>,
    par: Vec<Vec<usize>>,
}

impl FuzzyBuffers {
    fn new() -> Self {
        Self {
            tchars: Vec::with_capacity(256),
            qchars: Vec::with_capacity(64),
            dp_prev: Vec::with_capacity(256),
            dp_cur: Vec::with_capacity(256),
            par: Vec::with_capacity(64),
        }
    }
}

// 快速排斥：检查 query 的所有字符是否都在 text 中按序存在
fn quick_contains_chars(t_chars: &[char], q_chars: &[char]) -> bool {
    if q_chars.is_empty() { return true; }
    let mut qi = 0;
    for c in t_chars.iter() {
        if c.eq_ignore_ascii_case(&q_chars[qi]) {
            qi += 1;
            if qi == q_chars.len() { return true; }
        }
    }
    false
}

pub fn filter(
    items: &[&str],
    pinyin_cache: &HashMap<usize, PinyinData>,
    history: &HashMap<String, u32>,
    query: &str
) -> Vec<MatchResult> {
    if query.is_empty() {
        return items
            .iter()
            .enumerate()
            .map(|(idx, _)| MatchResult {
                original_idx: idx,
                score: 0,
                highlight_indices: HashSet::new(),
            })
            .collect();
    }

    let mut results = Vec::new();
    let q_chars: Vec<char> = query.chars().collect();

    for (idx, item) in items.iter().enumerate() {
        let mut best_score = i32::MIN;
        let mut hl = HashSet::new();

        // 优先级 1: 原文双向匹配
        if let Some((score, idxs)) = fuzzy_match_bidirectional(item, &q_chars) {
            best_score = score + 50; // 原文匹配给更高优先级
            hl.extend(idxs);
        }

        // 优先级 2: 拼音双向匹配
        if best_score == i32::MIN {
            if let Some(pinyin_data) = pinyin_cache.get(&idx) {
                if let Some((score, idxs)) = match_pinyin_cached(pinyin_data, &q_chars) {
                    best_score = score;
                    hl = idxs;
                }
            }
        }

        // 如果匹配成功，计算历史权重并加入结果
        if best_score > i32::MIN {
            // 历史频率加成：每次使用加 100 分，最高加 500 分（防止过度霸榜）
            let hist_bonus = history.get(*item).map_or(0, |&c| (c * 100).min(500)) as i32;
            let final_score = best_score + hist_bonus;

            results.push(MatchResult {
                original_idx: idx,
                score: final_score,
                highlight_indices: hl,
            });
        }
    }

    // Top-K 截断
    if results.len() > MAX_MATCHES {
        results.select_nth_unstable_by(MAX_MATCHES - 1, |a, b| b.score.cmp(&a.score));
        results.truncate(MAX_MATCHES);
        results.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    } else {
        results.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    }

    results
}

// 纯净的 DP 核心算法，接收字符切片和可变缓冲区，彻底零分配
fn run_dp(
    t_chars: &[char],
    q_chars: &[char],
    dp_prev: &mut Vec<i32>,
    dp_cur: &mut Vec<i32>,
    par: &mut Vec<Vec<usize>>
) -> Option<(i32, Vec<usize>)> {
    let qlen = q_chars.len();
    if qlen == 0 { return Some((0, Vec::new())); }

    if !quick_contains_chars(t_chars, q_chars) {
        return None;
    }

    let tlen = t_chars.len();
    if qlen > tlen { return None; }

    dp_prev.clear();
    dp_prev.resize(tlen, i32::MIN);
    dp_cur.clear();
    dp_cur.resize(tlen, i32::MIN);

    if par.len() < qlen {
        par.resize_with(qlen, Vec::new);
    }
    for i in 0..qlen {
        par[i].clear();
        par[i].resize(tlen, usize::MAX);
    }

    let boundary_bonus = |ti: usize| -> i32 {
        if ti == 0 || matches!(t_chars[ti - 1], '/' | '\\' | '-' | '_' | '.' | ' ') {
            30
        } else {
            0
        }
    };

    for ti in 0..tlen {
        if t_chars[ti].eq_ignore_ascii_case(&q_chars[0]) {
            let mut s = 10i32;
            if t_chars[ti] == q_chars[0] { s += 5; }
            s += boundary_bonus(ti);
            dp_prev[ti] = s;
        }
    }

    for qi in 1..qlen {
        dp_cur.iter_mut().for_each(|x| *x = i32::MIN);

        let mut best_nc = i32::MIN;
        let mut best_nc_idx = usize::MAX;

        for ti in 0..tlen {
            if ti >= 2 && dp_prev[ti - 2] > i32::MIN {
                if dp_prev[ti - 2] > best_nc {
                    best_nc = dp_prev[ti - 2];
                    best_nc_idx = ti - 2;
                }
            }

            if !t_chars[ti].eq_ignore_ascii_case(&q_chars[qi]) {
                continue;
            }

            let mut best = i32::MIN;
            let mut best_p = usize::MAX;

            if ti >= 1 && dp_prev[ti - 1] > i32::MIN {
                let s = dp_prev[ti - 1] + 20;
                if s > best { best = s; best_p = ti - 1; }
            }

            if best_nc > i32::MIN {
                let s = best_nc - 10;
                if s > best { best = s; best_p = best_nc_idx; }
            }

            if best > i32::MIN {
                if t_chars[ti] == q_chars[qi] { best += 5; }
                best += boundary_bonus(ti);
                dp_cur[ti] = best;
                par[qi][ti] = best_p;
            }
        }
        std::mem::swap(dp_prev, dp_cur);
    }

    let last = qlen - 1;
    let mut best_score = i32::MIN;
    let mut best_end = usize::MAX;
    for ti in 0..tlen {
        if dp_prev[ti] > best_score {
            best_score = dp_prev[ti];
            best_end = ti;
        }
    }
    if best_end == usize::MAX { return None; }

    let mut highlights = vec![best_end];
    let mut ti = best_end;
    for qi in (1..=last).rev() {
        ti = par[qi][ti];
        highlights.push(ti);
    }
    highlights.reverse();

    Some((best_score, highlights))
}

// 终极零分配双向匹配
fn fuzzy_match_bidirectional(text: &str, q_chars: &[char]) -> Option<(i32, HashSet<usize>)> {
    let mut best_score = i32::MIN;
    let mut best_hl = HashSet::new();

    FUZZY_BUF.with(|buf_cell| {
        let mut buf = buf_cell.borrow_mut();
        let FuzzyBuffers { tchars, qchars, dp_prev, dp_cur, par } = &mut *buf;

        // 1. 正向匹配：直接填充并切片
        tchars.clear();
        tchars.extend(text.chars());

        qchars.clear();
        qchars.extend(q_chars.iter().copied());

        if let Some((score, idxs)) = run_dp(tchars, qchars, dp_prev, dp_cur, par) {
            best_score = score;
            best_hl.extend(idxs);
        }

        // 2. 逆向匹配：直接在原地 reverse！零分配！
        tchars.reverse();
        qchars.reverse();

        if let Some((r_score, r_idxs)) = run_dp(tchars, qchars, dp_prev, dp_cur, par) {
            if r_score > best_score {
                best_score = r_score;
                let tlen = tchars.len();
                best_hl.clear();
                for i in r_idxs {
                    best_hl.insert(tlen - 1 - i);
                }
            }
        }
    });

    if best_hl.is_empty() { None } else { Some((best_score, best_hl)) }
}

// 修改：使用双向匹配，并将拼音索引映射回原始字符索引
fn match_pinyin_cached(data: &PinyinData, q_chars: &[char]) -> Option<(i32, HashSet<usize>)> {
    let mut best_score = i32::MIN;
    let mut highlights = HashSet::new();

    // 1. 尝试首字母双向匹配
    if !data.init.is_empty() {
        if let Some((score, idxs)) = fuzzy_match_bidirectional(&data.init, q_chars) {
            if score > best_score { best_score = score; }
            for i in idxs {
                if i < data.init_map.len() {
                    let orig = data.init_map[i];
                    if orig != usize::MAX {
                        highlights.insert(orig);
                    }
                }
            }
        }
    }

    // 2. 尝试全拼双向匹配
    if !data.full.is_empty() {
        if let Some((score, idxs)) = fuzzy_match_bidirectional(&data.full, q_chars) {
            if score > best_score { best_score = score; }
            for i in idxs {
                if i < data.full_map.len() {
                    let orig = data.full_map[i];
                    if orig != usize::MAX {
                        highlights.insert(orig);
                    }
                }
            }
        }
    }

    if highlights.is_empty() {
        None
    } else {
        Some((best_score, highlights))
    }
}
