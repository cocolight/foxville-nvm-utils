// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// eep_report.rs -- foxeep 的输出层：单文件详情、原始 word dump、逐 word 比对、帮助、JSON。
//
// 与 foxflash 同理：这里只在非 --json 模式被调用，可以放心 println!。

#![allow(dead_code)]

use crate::eep_parse::{compat_label, version_label, Info, Kind};
use crate::nvm::{devid_label, imgtype_label, known_eepid};
use crate::term::{line, thin_line};

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Eep => ".eep (Shadow RAM 转储)",
        Kind::Bin => ".bin (flash 镜像, 按 u16 小端取 word)",
    }
}

// ============================================================================
// 单文件详情
// ============================================================================

pub fn show_result(r: &Info) {
    let s = &r.src;
    println!("{}", line());
    println!("文件    : {}", s.name);
    println!("来源    : {}", s.path);
    println!("类型    : {}", kind_label(s.kind));
    println!(
        "大小    : {} 字节{}",
        s.file_bytes,
        if s.kind == Kind::Eep {
            format!("  /  {} 行 / {} word", s.text_lines, s.words.len())
        } else {
            format!("  / {} word", s.words.len())
        }
    );
    if !s.range_headers.is_empty() {
        println!(
            "分节头  : {} 个 Range 注释（官方 .eep 格式）",
            s.range_headers.len()
        );
    }
    println!("{}", thin_line());
    println!("MAC 地址    w0x00 : {}", r.mac);
    println!(
        "NVM_COMPAT  w0x03 : 0x{:04X}  高字节 0x{:02X} -> {}",
        r.compat,
        (r.compat >> 8) as u8,
        compat_label(r.compat)
    );
    println!(
        "NVM 版本    w0x05 : 0x{:04X}  -> {}",
        r.version,
        version_label(r.version)
    );
    println!(
        "镜像类型    w0x10 : 0x{:04X}  -> {}   (与 w0x03 互证；公开头文件里无名)",
        r.imgtype,
        imgtype_label(r.imgtype)
    );
    println!("{}", thin_line());
    println!(
        "PBA 板号        : {}",
        if r.pba.is_empty() { "<无>" } else { &r.pba }
    );
    println!("              {}", r.pba_note);
    println!(
        "SubDev w0x0B    : 0x{:04X}      SubVen w0x0C : 0x{:04X}",
        r.subdev, r.subven
    );
    println!(
        "DevID  w0x0D    : 0x{:04X} -> {}   VenID w0x0E : 0x{:04X}",
        r.devid,
        devid_label(r.devid),
        r.venid
    );
    println!("InitCtrl2 w0x0F : 0x{:04X}", r.init2);
    println!("{}", thin_line());
    println!(
        "NVM 校验和  w0..0x3F : 0x{:04X}  -> {}   (内核 NVM_SUM 应=0xBABA；补丁字 w0x3F=0x{:04X})",
        r.checksum,
        if r.checksum == crate::nvm::NVM_SUM {
            "OK"
        } else {
            "不符"
        },
        r.checksum_word
    );
    match &r.altmac {
        Some(m) => println!(
            "AlternateMAC       : {}   (经 w0x37 指针 0x{:04X} 定位)",
            m, r.altmac_ptr
        ),
        None => println!(
            "AlternateMAC       : <该区块已移除, 指针=0x{:04X}>",
            r.altmac_ptr
        ),
    }
    println!("EEPID/Etrack w0x42/43 : 0x{:08X}", r.etrack);
    let k = known_eepid(r.etrack);
    if !k.is_empty() {
        println!("             已知    : {}", k);
    }
    println!(
        "有效区间            : w0x000 .. w0x{:03X} 之后全为 0xFFFF",
        r.last_used
    );

    if !r.notes.is_empty() {
        println!("{}", thin_line());
        for n in &r.notes {
            println!("[!] {}", n);
        }
    }
}

/// `--dump 0x00-0x7f`：按 8 word/行打印原始内容
pub fn show_dump(r: &Info, start: usize, end: usize) {
    println!(
        "--- {}  word 0x{:03X} .. 0x{:03X} ---",
        r.src.name, start, end
    );
    let end = end.min(r.src.words.len().saturating_sub(1));
    let mut i = start;
    while i <= end {
        let mut row = format!("  w0x{:03X} : ", i);
        for k in 0..8 {
            if i + k <= end {
                let v = if i + k < r.src.words.len() {
                    r.src.words[i + k]
                } else {
                    0xFFFF
                };
                row.push_str(&format!("{:04X} ", v));
            } else {
                row.push_str("     ");
            }
        }
        println!("{}", row.trim_end());
        i += 8;
    }
}

// ============================================================================
// 逐 word 比对
// ============================================================================

pub fn show_compare(rs: &[Info]) {
    if rs.len() < 2 {
        return;
    }
    println!();
    println!("{}", line());
    println!("逐 word 比对（{} 个文件，按最短长度对齐）", rs.len());
    println!("{}", line());

    let base = &rs[0];
    for other in &rs[1..] {
        let n = base.src.words.len().min(other.src.words.len());
        let mut diffs: Vec<usize> = Vec::new();
        for i in 0..n {
            if base.src.words[i] != other.src.words[i] {
                diffs.push(i);
            }
        }
        println!();
        println!("{}  vs  {}", base.src.name, other.src.name);
        println!("  比对范围 : word 0x000 .. 0x{:03X}（{} 个 word）", n - 1, n);
        if diffs.is_empty() {
            println!("  结果     : 完全相同 ✅");
        } else {
            println!("  结果     : {} 个 word 不同", diffs.len());
            for i in diffs.iter().take(24) {
                println!(
                    "    w0x{:03X}   A=0x{:04X}   B=0x{:04X}",
                    i, base.src.words[*i], other.src.words[*i]
                );
            }
            if diffs.len() > 24 {
                println!("    ... 另有 {} 处未列出", diffs.len() - 24);
            }
        }
        if base.src.words.len() != other.src.words.len() {
            // .bin 通常比 .eep 长得多（1MB 镜像 = 524288 word），属正常，用 [i]；
            // 两边都在 Shadow RAM 量级却不等，才可能是文件被截断，用 [!]。
            let both_small = base.src.words.len() <= 0x1000 && other.src.words.len() <= 0x1000;
            if both_small {
                println!(
                    "  [!] 长度不同：A={} word，B={} word（两边都在 Shadow RAM 量级，可能文件被截断）",
                    base.src.words.len(),
                    other.src.words.len()
                );
            } else {
                println!(
                    "  [i] 长度不同：A={} word，B={} word，仅比对前 {} 个 word（.bin 通常远长于 .eep，属正常）",
                    base.src.words.len(),
                    other.src.words.len(),
                    n
                );
            }
        }
    }
}

// ============================================================================
// 帮助 / JSON
// ============================================================================

pub fn usage() {
    println!(
        "{} {} —— Intel I225/I226 Shadow RAM 转储（.eep）解析与比对",
        crate::APP,
        crate::VERSION
    );
    println!();
    println!("用法:");
    println!("  foxeep.exe <文件.eep> [文件2.eep|镜像.bin ...]   解析，多个则逐 word 比对");
    println!("  foxeep.exe <目录> [-r]                          扫描目录（只收 .eep）");
    println!("  foxeep.exe <文件.eep> --dump 0x00-0x7f          打印原始 word");
    println!("  foxeep.exe --json <文件.eep>                    机器可读输出（stdout 纯 JSON）");
    println!("  foxeep.exe -i                                   强制进入交互模式（等价于双击启动）");
    println!("  foxeep.exe --help                               本帮助");
    println!();
    println!("双击启动（无参数）:");
    println!("  双击 foxeep.exe 直接进入**交互模式**：把 .eep 拖进窗口或粘贴路径回车即可，");
    println!("  一行可以写多个（空格分隔），跑完会回到提示符等你继续，输入 exit 退出。");
    println!("  把 .eep 拖到 exe 图标上启动，跑完同样不会关窗，会转入交互模式。");
    println!();
    println!("说明:");
    println!("  .eep 是 eeupdate /DUMP 产出的**文本**文件，每行 8 个十六进制 word，");
    println!("  固定 0x800=2048 word（4KB Shadow RAM）；';' 开头为注释。");
    println!("  实测 .eep word i  ==  .bin 的 u16 小端 @字节 2i，故可直接与 .bin 比对。");
    println!();
    println!("  --dump 的范围写法：0x00-0x7f（十六进制）或 0-127（十进制）");
    println!("  同时给 .eep 和 .bin 时会自动按 word 对齐比对，列出所有不同的 word。");
}

fn json_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

pub fn print_json(rs: &[Info]) {
    println!("[");
    for (idx, r) in rs.iter().enumerate() {
        let s = &r.src;
        println!("  {{");
        println!("    \"file\": \"{}\",", json_escape(&s.name));
        println!("    \"path\": \"{}\",", json_escape(&s.path));
        println!(
            "    \"kind\": \"{}\",",
            if s.kind == Kind::Eep { "eep" } else { "bin" }
        );
        println!("    \"file_bytes\": {},", s.file_bytes);
        println!("    \"text_lines\": {},", s.text_lines);
        println!("    \"words\": {},", s.words.len());
        println!("    \"bad_tokens\": {},", s.bad_tokens);
        println!("    \"mac\": \"{}\",", r.mac);
        println!("    \"compat\": \"0x{:04X}\",", r.compat);
        println!("    \"capacity\": \"{}\",", compat_label(r.compat));
        println!("    \"nvmver\": \"0x{:04X}\",", r.version);
        println!("    \"nvmver_label\": \"{}\",", version_label(r.version));
        println!("    \"imgtype\": \"0x{:04X}\",", r.imgtype);
        println!("    \"subdev\": \"0x{:04X}\",", r.subdev);
        println!("    \"subven\": \"0x{:04X}\",", r.subven);
        println!("    \"devid\": \"0x{:04X}\",", r.devid);
        println!("    \"devid_label\": \"{}\",", devid_label(r.devid));
        println!("    \"venid\": \"0x{:04X}\",", r.venid);
        println!("    \"init2\": \"0x{:04X}\",", r.init2);
        println!("    \"pba\": \"{}\",", json_escape(&r.pba));
        println!(
            "    \"altmac\": \"{}\",",
            r.altmac.clone().unwrap_or_default()
        );
        println!("    \"checksum\": \"0x{:04X}\",", r.checksum);
        println!(
            "    \"checksum_ok\": {},",
            r.checksum == crate::nvm::NVM_SUM
        );
        println!("    \"eepid\": \"0x{:08X}\",", r.etrack);
        println!(
            "    \"eepid_note\": \"{}\",",
            json_escape(&known_eepid(r.etrack))
        );
        println!("    \"last_used\": {},", r.last_used);
        println!("    \"notes\": [");
        for (j, n) in r.notes.iter().enumerate() {
            let comma = if j + 1 == r.notes.len() { "" } else { "," };
            println!("      \"{}\"{}", json_escape(n), comma);
        }
        println!("    ]");
        println!("  }}{}", if idx + 1 == rs.len() { "" } else { "," });
    }
    println!("]");
}
