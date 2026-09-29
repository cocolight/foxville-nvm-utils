// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// foxflash.rs -- Intel I225 / I226 (Foxville) NVM 固件镜像离线体检工具
//                （入口：命令行参数解析 + 交互模式调度）
//
// ----------------------------------------------------------------------------
// BUILD（零第三方依赖，纯 Rust 标准库 + 多文件模块，**不需要 Cargo**）：
//
//     cd 仓库根目录
//     rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o src/foxflash.exe src/foxflash.rs
//
// 说明：本文件通过 `#[path = "lib/xxx.rs"] mod xxx;` 引入同目录 lib/ 下的模块。
// 路径是相对**本文件所在目录**（src/）解析的，所以从仓库根目录或 src/ 里编译都行。
// 交互模式与终端基座见 lib/repl.rs、lib/term.rs。
// ----------------------------------------------------------------------------
//
// 协议：GPL-3.0-or-later（见仓库根 LICENSE）
// 本项目源于「倍控 G31-1338 四口机 I225-V 固件升级」

// ---- 共用模块（foxeep 也用同一批文件，见各文件头注释）----
#[path = "lib/term.rs"]
mod term; // 终端基座：UTF-8 / 中文宽度 / note() / 双击检测
#[path = "lib/repl.rs"]
mod repl; // 双击启动的交互模式
#[path = "lib/nvm.rs"]
mod nvm; // 字段知识（两工具唯一数据源）

// ---- foxflash 专用模块 ----
#[path = "lib/md5.rs"]
mod md5; // 手写 MD5
#[path = "lib/deflate.rs"]
mod deflate; // 手写 DEFLATE / gzip
#[path = "lib/archive.rs"]
mod archive; // zip / tar(.gz) 直接读包
#[path = "lib/flash_parse.rs"]
mod flash_parse; // 输入收集 + .bin 解析
#[path = "lib/flash_report.rs"]
mod flash_report; // 排版输出

use std::env;
use std::process;

const APP: &str = "foxflash";
const VERSION: &str = "2.2 (rust)";
const SUMMARY: &str = "Intel I225/I226 (Foxville) NVM 镜像离线体检";
const PROMPT: &str = "foxflash> ";

// ============================================================================
// 一次任务：参数 -> 结果。返回退出码。
//
// 命令行模式与交互模式**共用**这一个函数，所以两条路径的行为天然一致。
// ============================================================================
fn run_once(args: &[String]) -> i32 {
    // 每次任务都从「非 json」起步：交互模式里上一次的 --json 不应影响下一次
    term::set_json_mode(false);

    let mut recursive = false;
    let mut json = false;
    let mut targets: Vec<String> = Vec::new();

    for a in args {
        match a.as_str() {
            "-r" | "--recursive" | "-R" => recursive = true,
            "--json" => {
                json = true;
                term::set_json_mode(true);
            }
            "--list-known" => {
                flash_report::show_known();
                return 0;
            }
            // 交互模式里打 help 更顺手，与 --help 等价
            "help" | "-h" | "--help" | "/?" => {
                flash_report::usage();
                return 0;
            }
            _ => targets.push(a.clone()),
        }
    }

    if targets.is_empty() {
        flash_report::usage();
        println!();
        println!("提示：什么都不带地双击 {} 图标会进入交互模式，把文件拖进窗口即可。", APP);
        return 1;
    }

    let entries = flash_parse::collect(&targets, recursive);
    if entries.is_empty() {
        return 1;
    }

    let results: Vec<flash_parse::ImageInfo> = entries
        .into_iter()
        .map(|e| flash_parse::parse_image(&e.name, &e.source, e.data))
        .collect();

    if json {
        flash_report::print_json(&results);
    } else {
        for r in &results {
            flash_report::show_result(r);
        }
        flash_report::show_compare(&results);
    }

    let bad = results.iter().filter(|r| !r.ok).count();
    if bad == results.len() {
        1
    } else {
        0
    }
}

// ============================================================================
// 入口
// ============================================================================
fn main() {
    term::set_console_utf8();
    let mut args: Vec<String> = env::args().skip(1).collect();

    // true = 双击 exe / 把文件拖到图标上（本进程独占一个新控制台）
    let own_console = term::launched_by_double_click();

    // 显式要求交互模式（`-i` / `--interactive`）。给两种人用：
    //   1) 在 cmd 里想手动进交互模式、又不想让 stdin 判定干扰的人；
    //   2) 回归测试（`printf '...\nexit\n' | foxflash.exe -i`）。
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
        // 被脚本 / 管道调用（stdin 不是终端）时保持原行为——打帮助 + 退出码 1，
        // 免得脚本挂在这里等输入。
        if own_console || term::stdin_is_terminal() {
            repl::run(APP, VERSION, SUMMARY, PROMPT, run_once);
            return;
        }
        process::exit(run_once(&args));
    }

    let code = run_once(&args);
    if own_console {
        // 拖拽 / 双击启动：跑完别把窗口关掉，转入交互模式继续看下一份
        repl::run(APP, VERSION, SUMMARY, PROMPT, run_once);
    }
    process::exit(code);
}
