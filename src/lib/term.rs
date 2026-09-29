// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// term.rs -- 终端基础层：控制台 UTF-8、中文显示宽度、提示输出口、按行读取、
//            「双击 / 拖拽启动」检测。
//
// 由 foxflash.rs / foxeep.rs 两个入口共用（各自用 #[path = "lib/term.rs"] 引入，
// 仍是**纯 rustc 构建、零第三方依赖**，也不引入 Cargo）。
//
// 之所以能共用：Rust 允许两个 crate 根各自把一个同名模块指向同一个源文件；
// 每个 exe 编译时会各带一份自己的副本（含下面那份 JSON_MODE 状态），互不干扰。

// 两个入口各自只用到本模块的一部分函数，未用到的在本 crate 里是死代码。
// 这是「共享模块」的固有现象，统一在这里放行，避免每次编译都刷一屏 warning。
#![allow(dead_code)]

use std::io::{self, BufRead, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

const UTF8_CODE_PAGE: u32 = 65001;

// ============================================================================
// 1. 提示输出口 + JSON 模式开关
// ============================================================================

/// `--json` 开关。开启后所有 [i]/[!] 提示改走 stderr，
/// 保证 stdout 是**纯 JSON**（否则提示行会混在 JSON 前面，严格解析器直接报错）。
static JSON_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_json_mode(on: bool) {
    JSON_MODE.store(on, Ordering::Relaxed);
}

pub fn json_mode() -> bool {
    JSON_MODE.load(Ordering::Relaxed)
}

/// 统一的提示输出口：正常模式走 stdout，--json 模式走 stderr。
///
/// **收集阶段的提示一律走这里**，不要直接 println!，否则会插到 JSON 前面。
/// （交互模式的横幅/提示走 repl.rs，那些只在非 --json 场景出现。）
pub fn note(msg: &str) {
    if json_mode() {
        eprintln!("{}", msg);
    } else {
        println!("{}", msg);
    }
}

// ============================================================================
// 2. 平台相关：强制 Windows 控制台走 UTF-8，否则中文会按 GBK 输出成乱码
// ============================================================================

#[cfg(windows)]
extern "system" {
    fn SetConsoleOutputCP(code_page: u32) -> i32;
    fn SetConsoleCP(code_page: u32) -> i32;
    fn GetConsoleProcessList(lpdw_process_list: *mut u32, dw_process_count: u32) -> u32;
}

pub fn set_console_utf8() {
    #[cfg(windows)]
    unsafe {
        SetConsoleOutputCP(UTF8_CODE_PAGE);
        SetConsoleCP(UTF8_CODE_PAGE);
    }
}

/// 判断「是不是双击 exe / 把文件拖到图标上」这种方式启动的。
///
/// 两个条件同时成立才算：
///
/// 1. **本进程独占一个控制台** —— `GetConsoleProcessList` 只数得到它自己（== 1）。
///    - 双击 / 拖拽：Windows 新开一个控制台，里面只有本进程 -> 1
///    - 从 cmd / PowerShell / .bat 启动：同一个控制台里还挂着父 shell -> >= 2
///    - stdin/stdout 被重定向、或在 mintty（Git Bash）里跑：根本没有控制台 -> 0
/// 2. **stdin 是终端**。
///
/// 为什么要加第 2 条：万一某个环境报出「独占控制台」但 stdin 其实是管道，
/// 交互模式会卡在那里等输入 —— 那会让调用它的脚本直接挂死。
/// 宁可漏判（双击时窗口照常关闭），也不能误判。第 2 条把这种风险堵死。
///
/// 注意用「控制台独占」而不是「有没有参数」来判定：拖拽**是有参数**的，
/// 但同样不希望窗口一闪就关。
pub fn launched_by_double_click() -> bool {
    #[cfg(windows)]
    unsafe {
        let mut buf = [0u32; 8];
        GetConsoleProcessList(buf.as_mut_ptr(), buf.len() as u32) == 1 && stdin_is_terminal()
    }
    #[cfg(not(windows))]
    {
        // 非 Windows 没有「双击弹控制台」这套语义
        false
    }
}

/// stdin 是不是一个交互式终端。无参数启动时用它区分「人坐在终端前」与「被脚本调用」。
pub fn stdin_is_terminal() -> bool {
    io::stdin().is_terminal()
}

// ============================================================================
// 3. 按行读取 + 提示符（交互模式用）
// ============================================================================

/// 读一行，去掉行尾的 `\r\n`。遇到 EOF（管道关闭、Ctrl+Z 回车）返回 None。
pub fn read_line() -> Option<String> {
    let mut s = String::new();
    match io::stdin().lock().read_line(&mut s) {
        Ok(0) => None,
        Ok(_) => Some(s.trim_end_matches(['\r', '\n']).to_string()),
        Err(_) => None,
    }
}

/// 打印提示符并立即 flush —— 否则提示符会卡在缓冲区里，用户看不到。
pub fn prompt(s: &str) {
    print!("{}", s);
    let _ = io::stdout().flush();
}

// ============================================================================
// 4. 显示宽度（中文等全角字符在等宽终端里占 2 列）
// ============================================================================

pub fn char_width(c: char) -> usize {
    let u = c as u32;
    if (0x1100..=0x115F).contains(&u) {
        2
    } else if (0x2E80..=0x303E).contains(&u) {
        2
    } else if (0x3041..=0x33FF).contains(&u) {
        2
    } else if (0x3400..=0x4DBF).contains(&u) {
        2
    } else if (0x4E00..=0x9FFF).contains(&u) {
        2
    } else if (0xA000..=0xA4CF).contains(&u) {
        2
    } else if (0xAC00..=0xD7A3).contains(&u) {
        2
    } else if (0xF900..=0xFAFF).contains(&u) {
        2
    } else if (0xFE30..=0xFE6F).contains(&u) {
        2
    } else if (0xFF00..=0xFF60).contains(&u) {
        2
    } else if (0xFFE0..=0xFFE6).contains(&u) {
        2
    } else if (0x20000..=0x3FFFD).contains(&u) {
        2
    } else {
        1
    }
}

pub fn display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

pub fn pad(s: &str, width: usize) -> String {
    let w = display_width(s);
    let mut out = String::with_capacity(s.len() + width);
    out.push_str(s);
    if w < width {
        for _ in 0..(width - w) {
            out.push(' ');
        }
    }
    out
}

pub fn truncate(s: &str, max: usize) -> String {
    if display_width(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = char_width(c);
        if w + cw > max.saturating_sub(1) {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('~');
    out
}

// ============================================================================
// 5. 分隔线
// ============================================================================

pub fn line() -> String {
    "=".repeat(72)
}

pub fn thin_line() -> String {
    "-".repeat(72)
}
