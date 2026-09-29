// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// repl.rs -- 双击启动的**交互模式**（REPL）。
//
// 为什么需要它：这两个 exe 的典型用法是「双击 -> 看一份镜像 -> 关掉 -> 再看另一份」。
// 没有交互模式时，双击会把窗口开着却什么都不做（无参数 = 打一段帮助就退），
// 每次看新文件都得重新拖一次图标。这里把它变成一个常驻的小控制台：
//
//   * 无参数双击启动     -> 直接进交互模式（输入 exit 退出）
//   * 拖文件到图标上启动 -> 先跑完这一次，然后**不关窗**，转入交互模式继续
//   * 从 cmd/PowerShell  -> 行为完全不变（跑完就退，不干扰脚本与管道）
//
// 每跑完一次都会回到提示符，所以不需要「频繁启停」。

#![allow(dead_code)]

use crate::term;

/// 把一行输入拆成参数表。
///
/// 支持用双引号 / 单引号把带空格的路径包起来 —— Windows 把文件拖进控制台时，
/// 带空格的路径正是以 `"C:\path with space\a.bin"` 这种形式吐进来的。
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut has = false; // 当前 token 是否已经开始（区分空串 "" 与没有 token）
    let mut quote: Option<char> = None;

    for c in line.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    has = true;
                } else if c.is_whitespace() {
                    if has {
                        out.push(std::mem::take(&mut cur));
                        has = false;
                    }
                } else {
                    cur.push(c);
                    has = true;
                }
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

/// 交互模式主循环。
///
/// - `app` / `version` / `summary`：横幅
/// - `prompt`：提示符（形如 `foxflash> `）
/// - `handle`：一次任务的处理函数，返回进程退出码（交互模式下忽略）
///
/// 结束条件：用户输入 exit/quit/q，或 stdin 到 EOF（管道关闭、Ctrl+Z 回车）。
pub fn run<F>(app: &str, version: &str, summary: &str, prompt: &str, mut handle: F)
where
    F: FnMut(&[String]) -> i32,
{
    println!();
    println!("{}", term::line());
    println!("  {} {}   {}", app, version, summary);
    println!("{}", term::line());
    println!("把文件拖进这个窗口，或直接粘贴路径，回车即可；一行可以写多个，用空格分隔。");
    println!("输入 help 查看完整用法，输入 exit 退出。");
    println!();

    loop {
        term::prompt(prompt);
        let Some(raw) = term::read_line() else {
            // stdin 到 EOF：管道跑完 / Ctrl+Z 回车 / 窗口被关。安静收场。
            println!();
            break;
        };
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        let args = split_args(line);
        if args.is_empty() {
            continue;
        }

        let cmd = args[0].to_ascii_lowercase();
        if matches!(cmd.as_str(), "exit" | "quit" | "q" | ":q" | ":quit") {
            println!("已退出。");
            break;
        }

        // help 类命令由各工具自己的 usage() 处理，不重复维护一份帮助文本
        let is_help = matches!(cmd.as_str(), "help" | "?" | "-h" | "--help" | "/?");
        let _ = handle(&args);

        if !is_help {
            println!();
            println!("── 本次任务结束：继续输入文件路径即可再跑一次，输入 exit 退出 ──");
        }
        println!();
    }
}
