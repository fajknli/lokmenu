// src/matcher.rs

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use crate::pinyin::PinyinData;

const MAX_MATCHES: usize = 500;

pub struct MatchResult {
    pub original_idx: usize,
    pub score: i32,
    // 修改：改为 HashSet，用于合并首字母和全拼的匹配结果
    pub highlight_indices: HashSet<usize>,
}

// 线程局部内存池：避免每次匹配时的 Vec 堆分配
thread_local! {
    static FUZZY_BUF: RefCell<FuzzyBuffers> = RefCell::new(FuzzyBuffers::new());
}

struct FuzzyBuffers {
    tchars: Vec<char>,
    dp_prev: Vec<i32>,
    dp_cur: Vec<i32>,
    par: Vec<Vec<usize>>,
}

impl FuzzyBuffers {
    fn new() -> Self {
        Self {
            tchars: Vec::with_capacity(256),
            dp_prev: Vec::with_capacity(256),
            dp_cur: Vec::with_capacity(256),
            par: Vec::with_capacity(32),
        }
    }
}

// 快速排斥：检查 query 的所有字符是否都在 text 中按序存在
fn quick_contains(text: &str, q_chars: &[char]) -> bool {
    if q_chars.is_empty() { return true; }
    let mut qi = 0;
    for c in text.chars() {
        if c.eq_ignore_ascii_case(&q_chars[qi]) {
            qi += 1;
            if qi == q_chars.len() { return true; }
        }
    }
    false
}

// 修改函数签名，接收无锁的 HashMap
pub fn filter(
    items: &[&str],
    pinyin_cache: &HashMap<usize, PinyinData>,
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
        // 优先级 1: 原文匹配
        if let Some(res) = fuzzy_match(item, &q_chars) {
            let mut hl = HashSet::new();
            hl.extend(res.1);
            results.push(MatchResult {
                original_idx: idx,
                score: res.0 + 50, // 原文匹配给更高优先级
                highlight_indices: hl,
            });
            continue;
        }

        // 优先级 2: 拼音匹配
        // 因为 state.rs 启动时已经预热了缓存，这里直接读
        if let Some(pinyin_data) = pinyin_cache.get(&idx) {
            if let Some(res) = match_pinyin_cached(pinyin_data, &q_chars) {
                results.push(MatchResult {
                    original_idx: idx,
                    score: res.0,
                    highlight_indices: res.1,
                });
            }
        }
    }

    // Top-K 截断：极速提取前 500 名
    if results.len() > MAX_MATCHES {
        results.select_nth_unstable_by(MAX_MATCHES - 1, |a, b| b.score.cmp(&a.score));
        results.truncate(MAX_MATCHES);
        results.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    } else {
        results.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    }

    results
}

pub fn fuzzy_match(text: &str, q_chars: &[char]) -> Option<(i32, Vec<usize>)> {
    let qlen = q_chars.len();
    if qlen == 0 { return Some((0, Vec::new())); }

    if !quick_contains(text, q_chars) {
        return None;
    }

    FUZZY_BUF.with(|buf_cell| {
        let mut buf = buf_cell.borrow_mut();
        let FuzzyBuffers { tchars, dp_prev, dp_cur, par } = &mut *buf;

        tchars.clear();
        tchars.extend(text.chars());
        let tlen = tchars.len();

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
            if ti == 0 || matches!(tchars[ti - 1], '/' | '\\' | '-' | '_' | '.' | ' ') {
                30
            } else {
                0
            }
        };

        for ti in 0..tlen {
            if tchars[ti].eq_ignore_ascii_case(&q_chars[0]) {
                let mut s = 10i32;
                if tchars[ti] == q_chars[0] { s += 5; }
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

                if !tchars[ti].eq_ignore_ascii_case(&q_chars[qi]) {
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
                    if tchars[ti] == q_chars[qi] { best += 5; }
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
    })
}

// 修改：使用映射表将拼音索引转换为原始字符索引，并合并首字母与全拼的匹配结果
fn match_pinyin_cached(data: &PinyinData, q_chars: &[char]) -> Option<(i32, HashSet<usize>)> {
    let mut best_score = i32::MIN;
    let mut highlights = HashSet::new();

    // 1. 尝试首字母匹配
    if !data.init.is_empty() {
        if let Some((score, idxs)) = fuzzy_match(&data.init, q_chars) {
            if score > best_score { best_score = score; }
            for i in idxs {
                if i < data.init_map.len() {
                    let orig = data.init_map[i];
                    // 过滤掉隐形墙的占位符 usize::MAX
                    if orig != usize::MAX {
                        highlights.insert(orig);
                    }
                }
            }
        }
    }

    // 2. 尝试全拼匹配
    if !data.full.is_empty() {
        if let Some((score, idxs)) = fuzzy_match(&data.full, q_chars) {
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

    // 3. 如果有匹配结果，返回最高得分和并集高亮
    if highlights.is_empty() {
        None
    } else {
        Some((best_score, highlights))
    }
}
