// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// foxeep.rs -- Intel I225 / I226 (Foxville) Shadow RAM 转储（*.eep）解析与比对工具
//              （入口：命令行参数解析 + 交互模式调度）
//
// 姊妹工具：
//     foxflash.exe  解析完整 flash 镜像（.bin / .zip / .tar.gz）
//     foxeep.exe    解析 Shadow RAM 转储（.eep，eeupdate /DUMP 的产物）
//
// ----------------------------------------------------------------------------
// BUILD（零第三方依赖，纯 Rust 标准库 + 多文件模块，**不需要 Cargo**）：
//
//     cd 仓库根目录
//     rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o src/foxeep.exe src/foxeep.rs
//
// 说明：本文件通过 `#[path = "lib/xxx.rs"] mod xxx;` 引入同目录 lib/ 下的模块，
// 其中 term / repl / nvm 三个与 foxflash **共用同一份源文件**——
// 字段表、版本解码、EEPID 对照表从此只有一处定义，不会再出现「改一边忘一边」。
// ----------------------------------------------------------------------------
//
// 协议：GPL-3.0-or-later（见仓库根 LICENSE）
// 作者：为「倍控 G31-1338 四口机 I225-V 固件升级」项目而写

// ---- 共用模块（foxflash 也用同一批文件）----
#[path = "lib/term.rs"]
mod term; // 终端基座：UTF-8 / note() / 双击检测
#[path = "lib/repl.rs"]
mod repl; // 双击启动的交互模式
#[path = "lib/nvm.rs"]
mod nvm; // 字段知识（两工具唯一数据源）

// ---- foxeep 专用模块 ----
#[path = "lib/eep_parse.rs"]
mod eep_parse; // .eep 文本 / .bin 二进制的 word 还原 + 体检
#[path = "lib/eep_report.rs"]
mod eep_report; // 排版输出

use std::env;
use std::process;

const APP: &str = "foxeep";
const VERSION: &str = "1.1 (rust)";
const SUMMARY: &str = "Intel I225/I226 Shadow RAM 转储（.eep）解析与比对";
const PROMPT: &str = "foxeep> ";

// ============================================================================
// 一次任务：参数 -> 结果。返回退出码。
//
// 命令行模式与交互模式**共用**这一个函数，两条路径行为一致。
// ============================================================================
fn run_once(args: &[String]) -> i32 {
    // 每次任务都从「非 json」起步，避免交互模式里上一次的 --json 影响下一次
    term::set_json_mode(false);

    let mut recursive = false;
    let mut json = false;
    let mut dump: Option<(usize, usize)> = None;
    let mut targets: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "-r" | "--recursive" | "-R" => recursive = true,
            "--json" => {
                json = true;
                term::set_json_mode(true);
            }
            "--dump" => match args.get(i + 1) {
                Some(v) => match eep_parse::parse_range(v) {
                    Some(r) => {
                        dump = Some(r);
                        i += 1; // 吃掉范围参数
                    }
                    None => {
                        eprintln!("[!] --dump 范围无法解析：{}（示例 0x00-0x7f）", v);
                        return 2;
                    }
                },
                None => {
                    eprintln!("[!] --dump 后面缺少范围参数（示例 0x00-0x7f）");
                    return 2;
                }
            },
            // 交互模式里打 help 更顺手，与 --help 等价
            "help" | "-h" | "--help" | "/?" => {
                eep_report::usage();
                return 0;
            }
            _ => targets.push(args[i].clone()),
        }
        i += 1;
    }

    if targets.is_empty() {
        eep_report::usage();
        println!();
        println!("提示：什么都不带地双击 {} 图标会进入交互模式，把文件拖进窗口即可。", APP);
        return 1;
    }

    let srcs = eep_parse::collect(&targets, recursive);
    if srcs.is_empty() {
        return 1;
    }

    let results: Vec<eep_parse::Info> = srcs.into_iter().map(eep_parse::analyze).collect();

    if json {
        eep_report::print_json(&results);
        return 0;
    }

    for r in &results {
        eep_report::show_result(r);
        if let Some((a, b)) = dump {
            eep_report::show_dump(r, a, b);
        }
    }
    eep_report::show_compare(&results);

    if results.len() == 1 && dump.is_none() {
        println!();
        println!("提示：再给一个 .eep 或 .bin 就能逐 word 比对（例：foxeep.exe a.eep b.bin）");
    }
    0
}

// ============================================================================
// 入口
// ============================================================================
fn main() {
    term::set_console_utf8();
    let mut args: Vec<String> = env::args().skip(1).collect();

    // true = 双击 exe / 把文件拖到图标上（本进程独占一个新控制台）
    let own_console = term::launched_by_double_click();

    // 显式要求交互模式（`-i` / `--interactive`），用途见 foxflash.rs 同处注释
    if let Some(pos) = args.iter().position(|a| a == "-i" || a == "--interactive") {
        args.remove(pos);
        if !args.is_empty() {
            let _ = run_once(&args); // 命令行给的参数先照常跑一遍
            println!();
        }
        repl::run(APP, VERSION, SUMMARY, PROMPT, run_once);
        return;
    }

    if args.is_empty() {
        // 无参数：人坐在终端前（或双击）就进交互模式；
        // 被脚本 / 管道调用时保持原行为——打帮助 + 退出码 1。
        if own_console || term::stdin_is_terminal() {
            repl::run(APP, VERSION, SUMMARY, PROMPT, run_once);
            return;
        }
        process::exit(run_once(&args));
    }

    let code = run_once(&args);
    if own_console {
        // 拖拽 / 双击启动：跑完不关窗，转入交互模式继续
        repl::run(APP, VERSION, SUMMARY, PROMPT, run_once);
    }
    process::exit(code);
}
