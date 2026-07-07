// src/matcher.rs

use std::cell::RefCell;

const MAX_MATCHES: usize = 500;

pub struct MatchResult {
    pub original_idx: usize,
    pub score: i32,
    pub highlight_indices: Vec<usize>,
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

pub fn filter(items: &[&str], pinyin_cache: &[(String, String)], query: &str) -> Vec<MatchResult> {
    if query.is_empty() {
        return items
            .iter()
            .enumerate()
            .map(|(idx, _)| MatchResult {
                original_idx: idx,
                score: 0,
                highlight_indices: Vec::new(),
            })
            .collect();
    }

    let mut results = Vec::new();
    let q_chars: Vec<char> = query.chars().collect();

    for (idx, item) in items.iter().enumerate() {
        // 优先级 1: 原文匹配
        if let Some(res) = fuzzy_match(item, &q_chars) {
            results.push(MatchResult {
                original_idx: idx,
                score: res.0 + 1000,
                highlight_indices: res.1,
            });
            continue;
        }

        // 优先级 2: 拼音匹配 (跳过空缓存)
        let (full, initials) = &pinyin_cache[idx];
        if !initials.is_empty() || !full.is_empty() {
            if let Some(res) = match_pinyin_cached(full, initials, &q_chars) {
                results.push(MatchResult {
                    original_idx: idx,
                    score: res.0,
                    highlight_indices: res.1,
                });
            }
        }
    }

    // Top-K 截断：极速提取前 500 名 (原地截断，零额外内存分配)
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

    // 1. 快速排斥：如果连子序列都不是，直接跳过，避免分配任何 DP 内存
    if !quick_contains(text, q_chars) {
        return None;
    }

    FUZZY_BUF.with(|buf_cell| {
        let mut buf = buf_cell.borrow_mut();

        // 解构借用，允许同时可变借用不同字段
        let FuzzyBuffers { tchars, dp_prev, dp_cur, par } = &mut *buf;

        // 2. 复用内存池，仅清理和重置长度，不重新分配堆内存
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

        // 初始化 qi = 0
        for ti in 0..tlen {
            if tchars[ti].eq_ignore_ascii_case(&q_chars[0]) {
                let mut s = 10i32;
                if tchars[ti] == q_chars[0] { s += 5; }
                s += boundary_bonus(ti);
                dp_prev[ti] = s;
            }
        }

        // 3. DP 递推求全局最优解
        for qi in 1..qlen {
            dp_cur.iter_mut().for_each(|x| *x = i32::MIN);

            let mut best_nc = i32::MIN;
            let mut best_nc_idx = usize::MAX;

            for ti in 0..tlen {
                // 维护滑动窗口：max(dp[qi-1][0..ti-2])
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

                // 选择 1：从 ti-1 连续接过来
                if ti >= 1 && dp_prev[ti - 1] > i32::MIN {
                    let s = dp_prev[ti - 1] + 20;
                    if s > best { best = s; best_p = ti - 1; }
                }

                // 选择 2：从更早的位置跳过来（有断点）
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

        // 找最优终点
        let last = qlen - 1;
        let mut best_score = i32::MIN;
        let mut best_end = usize::MAX;
        for ti in 0..tlen {
            // 注意：因为上面 swap 了，所以最后一层的结果在 dp_prev 里
            if dp_prev[ti] > best_score {
                best_score = dp_prev[ti];
                best_end = ti;
            }
        }
        if best_end == usize::MAX { return None; }

        // 回溯还原匹配位置
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

fn match_pinyin_cached(full: &str, initials: &str, q_chars: &[char]) -> Option<(i32, Vec<usize>)> {
    if !initials.is_empty() {
        if let Some(res) = fuzzy_match(initials, q_chars) {
            return Some((res.0 - 10, res.1));
        }
    }

    if !full.is_empty() {
        if let Some(res) = fuzzy_match(full, q_chars) {
            return Some((res.0 - 20, res.1));
        }
    }

    None
}
