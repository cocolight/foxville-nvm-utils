// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// eep_parse.rs -- foxeep 的解析层：把 .eep（文本）与 .bin（二进制）统一还原成
//                 word 数组，再按内核具名常量解出字段并做体检。
//
// ----------------------------------------------------------------------------
// .eep 是什么（实测归纳，2026-09-28）
// ----------------------------------------------------------------------------
// eeupdate 的官方说明（eeupdate.txt）：
//     /DUMP   "Dumps EEPROM/Shadow RAM contents to a *.eep file.
//              Dumps flash (if present) to a *.bin file"
//
// 实测 .eep 是**纯文本**，不是二进制：
//
//     官方包里的写法（带分节注释，11583 字节 / 320 行）：
//         ;
//         ;-------Range [0x00-0x3f]--------------
//         8C8C FBAA 7839 0D20 FFFF 1089 FFFF FFFF
//         FAFA 0125 602F 22D8 17AA 15F3 8086 8200
//         ...
//     本机 eeupdate /DUMP 出来的写法（无注释，10496 字节 / 256 行）：
//         BE60 02B4 0E68 0D20 FFFF 1057 FFFF FFFF
//         FAFA 0125 602F 0000 8086 15F3 8086 8200
//
// 规律：
//   * 每行 8 个 word，word 是 **4 位十六进制、按数值书写**（不是字节序原样）；
//     实测 .bin 前 6 字节 60 BE B4 02 68 0E，对应 .eep 前三 word
//     "BE60 02B4 0E68"：0xBE60 拆成字节是(小端) 60 BE，正好是 .bin 的 byte0..1。
//     所以 **.eep word i  ==  .bin 的 u16le @byte 2i**，与内核 "word addressable,
//     little-endian" 一致。
//   * 总长固定 **0x800 = 2048 word**（4 KB Shadow RAM），8 word/行 → 256 数据行。
//   * ';' 开头是注释；官方包用 ";-------Range [0x??-0x??]-----" 每 64 word 分一节。
//
// 实证：本机 60BEB402680E.eep 与同名 .bin 的 2048 个 word **零差异**，
//       两边 NVM 校验和都是 0xBABA。

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use crate::nvm::{
    capacity_from_compat_hi, devid_label, nvm_version_label, NVM_ETRACK_VALID, NVM_PBA_PTR_GUARD,
    NVM_SUM, SHADOW_RAM_WORDS,
};
use crate::term::note;

// ============================================================================
// 1. 源数据
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Eep,
    Bin,
}

pub struct Src {
    pub name: String,
    pub path: String,
    pub kind: Kind,
    pub file_bytes: usize,
    pub text_lines: usize,
    pub words: Vec<u16>,
    /// 官方 .eep 里的 ";-------Range [0x00-0x3f]-----" 分节头：(声明起始 word, 声明结束, 实际 word 数)
    pub range_headers: Vec<(usize, usize, usize)>,
    pub bad_tokens: usize,
}

/// 单个 word token -> u16。容忍 "0x" 前缀和 1~4 位十六进制。
fn parse_word(tok: &str) -> Option<u16> {
    let t = tok.trim();
    let t = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    if t.is_empty() || t.len() > 4 {
        return None;
    }
    u16::from_str_radix(t, 16).ok()
}

/// 从 ";-------Range [0x00-0x3f]--------------" 里抠出 (start, end)
fn parse_range_header(l: &str) -> Option<(usize, usize)> {
    let lb = l.find('[')?;
    let rb = l.find(']')?;
    if rb < lb {
        return None;
    }
    let inner = &l[lb + 1..rb];
    let dash = inner.find('-')?;
    let a = inner[..dash].trim_start_matches("0x");
    let b = inner[dash + 1..].trim_start_matches("0x");
    let a = usize::from_str_radix(a, 16).ok()?;
    let b = usize::from_str_radix(b, 16).ok()?;
    Some((a, b))
}

fn parse_eep_text(name: &str, path: &str, text: &str, file_bytes: usize) -> Src {
    let mut words: Vec<u16> = Vec::new();
    let mut headers: Vec<(usize, usize, usize)> = Vec::new();
    let mut bad = 0usize;
    let mut lines = 0usize;

    for raw in text.lines() {
        let l = raw.trim(); // 同时吃掉 \r
        if l.is_empty() {
            continue;
        }
        lines += 1;
        if l.starts_with(';') {
            if let Some((a, b)) = parse_range_header(l) {
                headers.push((a, b, words.len()));
            }
            continue;
        }
        for tok in l.split_whitespace() {
            match parse_word(tok) {
                Some(w) => words.push(w),
                None => bad += 1,
            }
        }
    }

    Src {
        name: name.to_string(),
        path: path.to_string(),
        kind: Kind::Eep,
        file_bytes,
        text_lines: lines,
        words,
        range_headers: headers,
        bad_tokens: bad,
    }
}

fn parse_bin(name: &str, path: &str, data: &[u8]) -> Src {
    let mut words = Vec::with_capacity(data.len() / 2);
    let mut i = 0;
    while i + 1 < data.len() {
        words.push(u16::from_le_bytes([data[i], data[i + 1]]));
        i += 2;
    }
    Src {
        name: name.to_string(),
        path: path.to_string(),
        kind: Kind::Bin,
        file_bytes: data.len(),
        text_lines: 0,
        words,
        range_headers: Vec::new(),
        bad_tokens: 0,
    }
}

// ============================================================================
// 2. word 空间取字段
// ============================================================================

/// 越界一律当 0xFFFF（与 Shadow RAM 未编程区一致），避免到处判边界。
fn word(w: &[u16], i: usize) -> u16 {
    if i < w.len() {
        w[i]
    } else {
        0xFFFF
    }
}

/// 按内核 igb/e1000e 的规则取 MAC：每个 word 先低字节后高字节（= 内存顺序）
/// igb_read_mac_addr(): mac[i] = nvm_data & 0xFF; mac[i+1] = nvm_data >> 8;
fn mac_from_words(w: &[u16], start: usize) -> String {
    let mut out = String::new();
    for k in 0..3 {
        let v = word(w, start + k);
        let lo = (v & 0xFF) as u8;
        let hi = (v >> 8) as u8;
        if k > 0 {
            out.push(':');
        }
        out.push_str(&format!("{:02X}:{:02X}", lo, hi));
    }
    out
}

/// PBA 板号。算法取自内核 igb_read_pba_string_generic()：
///     read(0x08) 必须 == 0xFAFA (NVM_PBA_PTR_GUARD)
///     ptr  = read(0x09)；ptr == 0xFFFF/0x0000 -> 无效
///     len  = read(ptr) ；len == 0xFFFF/0x0000 -> 无效
///     ptr++, len--
///     逐 word 取字符串
/// 字节序：内核写的是 (w>>8) 优先，但在 Foxville 上按该顺序解出的是乱码
/// （"G23456-000"），按内存顺序（低字节优先）才是 "2G43650-00"。故此处用内存顺序。
fn pba_string(w: &[u16]) -> (String, String) {
    let guard = word(w, 0x08);
    if guard != NVM_PBA_PTR_GUARD {
        return (
            String::new(),
            format!("守卫 w0x08=0x{:04X} != 0xFAFA，PBA 区无效", guard),
        );
    }
    let ptr = word(w, 0x09) as usize;
    if ptr == 0xFFFF || ptr == 0 {
        return (String::new(), format!("指针 w0x09=0x{:04X}，PBA 区无效", ptr));
    }
    if ptr >= w.len() {
        return (String::new(), format!("指针 w0x09=0x{:04X} 越界", ptr));
    }
    let len = word(w, ptr) as usize;
    if len == 0xFFFF || len == 0 {
        return (
            String::new(),
            format!("长度字 w0x{:03X}=0x{:04X}，PBA 区未编程", ptr, len),
        );
    }
    let mut s = String::new();
    for k in 0..(len - 1) {
        let v = word(w, ptr + 1 + k);
        let lo = (v & 0xFF) as u8;
        let hi = (v >> 8) as u8;
        if lo == 0 || hi == 0 || lo == 0xFF && hi == 0xFF {
            break;
        }
        s.push(lo as char);
        s.push(hi as char);
    }
    (
        s.clone(),
        format!("指针 w0x09=0x{:04X}  长度字=0x{:04X}", ptr, len),
    )
}

// ============================================================================
// 3. 分析 + 体检
// ============================================================================

pub struct Info {
    pub src: Src,
    pub mac: String,
    pub compat: u16,
    pub version: u16,
    pub imgtype: u16,
    pub subdev: u16,
    pub subven: u16,
    pub devid: u16,
    pub venid: u16,
    pub init2: u16,
    pub altmac: Option<String>,
    pub altmac_ptr: u16,
    pub checksum: u16,
    pub checksum_word: u16,
    pub etrack: u32,
    pub pba: String,
    pub pba_note: String,
    pub last_used: usize,
    pub notes: Vec<String>,
}

pub fn analyze(s: Src) -> Info {
    let w = &s.words;
    let mut notes: Vec<String> = Vec::new();

    if s.words.len() != SHADOW_RAM_WORDS {
        notes.push(format!(
            "word 数 {} != 期望 {}（eeupdate /DUMP 固定产出 0x800 word）",
            s.words.len(),
            SHADOW_RAM_WORDS
        ));
    }
    if s.bad_tokens > 0 {
        notes.push(format!(
            "有 {} 个无法解析的 token（非 1~4 位十六进制）",
            s.bad_tokens
        ));
    }
    for (decl, _end, actual) in &s.range_headers {
        if *decl != *actual {
            notes.push(format!(
                "分节头 Range 声明起点 0x{:X}，但此处实际已累计 0x{:X} 个 word（文件可能被手工编辑过）",
                decl, actual
            ));
        }
    }

    let checksum: u16 = (0..=0x3F).fold(0u32, |acc, i| acc + word(w, i) as u32) as u16;
    if checksum != NVM_SUM {
        notes.push(format!(
            "NVM 校验和 sum(w0..0x3F)=0x{:04X} != 0xBABA（内核 NVM_SUM）",
            checksum
        ));
    }
    let venid = word(w, 0x0E);
    if venid != 0x8086 {
        notes.push(format!(
            "VendorID w0x0E=0x{:04X} != 0x8086，可能不是 Intel NVM",
            venid
        ));
    }
    let devid = word(w, 0x0D);
    if devid_label(devid) == "未知" {
        notes.push(format!(
            "DeviceID w0x0D=0x{:04X} 不在 Foxville 已知表里（15F3/15F2/125B/125C/125D）",
            devid
        ));
    }
    let imgtype = word(w, 0x10);
    if imgtype != 0x8022 && imgtype != 0x80A2 {
        notes.push(format!(
            "w0x10=0x{:04X} 不是观测到的 0x8022(1MB)/0x80A2(2MB)。该字段在公开头文件里无名，\
             语义未确认；容量请以 w0x03 为准",
            imgtype
        ));
    }

    let etrack = ((word(w, 0x43) as u32) << 16) | word(w, 0x42) as u32;
    if etrack & 0x80000000 == 0 {
        notes.push(format!(
            "EtrackID 0x{:08X} 高位不是 0x{:04X}（内核 NVM_ETRACK_VALID），字段可能无效",
            etrack, NVM_ETRACK_VALID
        ));
    }

    let altmac_ptr = word(w, 0x37);
    let altmac = if altmac_ptr == 0xFFFF || altmac_ptr == 0 {
        None
    } else if (altmac_ptr as usize) + 3 > w.len() {
        notes.push(format!("AltMAC 指针 w0x37=0x{:04X} 越界", altmac_ptr));
        None
    } else {
        Some(mac_from_words(w, altmac_ptr as usize))
    };

    let (pba, pba_note) = pba_string(w);

    // 最后一个非 0xFFFF 的 word（Shadow RAM 的"用到哪儿了"）
    let last_used = w
        .iter()
        .enumerate()
        .filter(|(_, v)| **v != 0xFFFF)
        .map(|(i, _)| i)
        .last()
        .unwrap_or(0);

    Info {
        mac: mac_from_words(w, 0x00),
        compat: word(w, 0x03),
        version: word(w, 0x05),
        imgtype,
        subdev: word(w, 0x0B),
        subven: word(w, 0x0C),
        devid,
        venid,
        init2: word(w, 0x0F),
        altmac,
        altmac_ptr,
        checksum,
        checksum_word: word(w, 0x3F),
        etrack,
        pba,
        pba_note,
        last_used,
        notes,
        src: s,
    }
}

/// `w0x03` 的容量标签（与 foxflash 看 byte 0x07 走同一个函数）
pub fn compat_label(w: u16) -> String {
    capacity_from_compat_hi((w >> 8) as u8)
}

/// 版本字解码（与 foxflash 同源）
pub fn version_label(w: u16) -> String {
    nvm_version_label(w)
}

// ============================================================================
// 4. 输入收集
// ============================================================================

fn is_eep_name(n: &str) -> bool {
    n.to_ascii_lowercase().ends_with(".eep")
}

fn collect_from_dir(dir: &Path, recursive: bool) -> (Vec<Src>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0usize;
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if recursive {
                    let (sub, sk) = collect_from_dir(&p, true);
                    out.extend(sub);
                    skipped += sk;
                }
            } else {
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if is_eep_name(&name) {
                    if let Ok(raw) = fs::read(&p) {
                        let text = String::from_utf8_lossy(&raw).to_string();
                        out.push(parse_eep_text(&name, &p.to_string_lossy(), &text, raw.len()));
                        continue;
                    }
                }
                skipped += 1;
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    (out, skipped)
}

fn load_one(t: &str) -> Option<Src> {
    let path = PathBuf::from(t);
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let lower = t.to_ascii_lowercase();
    let raw = fs::read(&path).ok()?;
    if lower.ends_with(".eep") || lower.ends_with(".txt") {
        let text = String::from_utf8_lossy(&raw).to_string();
        Some(parse_eep_text(&name, t, &text, raw.len()))
    } else {
        // 其余一律当二进制（.bin/.img/.dat/无扩展名），按 u16 小端取 word
        Some(parse_bin(&name, t, &raw))
    }
}

pub fn collect(targets: &[String], recursive: bool) -> Vec<Src> {
    let mut out = Vec::new();
    for t in targets {
        let path = PathBuf::from(t);
        if path.is_dir() {
            let (got, skipped) = collect_from_dir(&path, recursive);
            if got.is_empty() {
                note(&format!("[!] 目录里没有 .eep 文件：{}", t));
            } else if skipped > 0 {
                note(&format!("[i] {}  ->  已跳过 {} 个非 .eep 的文件", t, skipped));
            }
            out.extend(got);
        } else if path.is_file() {
            match load_one(t) {
                Some(s) => out.push(s),
                None => note(&format!("[!] 读取失败：{}", t)),
            }
        } else {
            note(&format!("[!] 路径不存在：{}", t));
        }
    }
    out
}

// ============================================================================
// 5. --dump 的范围参数
// ============================================================================

/// 解析 --dump 的范围：支持 "0x00-0x7f" / "0-127" / 单个 "0x40"
pub fn parse_range(s: &str) -> Option<(usize, usize)> {
    let s = s.trim();
    if let Some(dash) = s.find('-') {
        let a = parse_num(s[..dash].trim())?;
        let b = parse_num(s[dash + 1..].trim())?;
        Some((a, b))
    } else {
        let a = parse_num(s)?;
        Some((a, a))
    }
}

pub fn parse_num(s: &str) -> Option<usize> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        usize::from_str_radix(rest, 16).ok()
    } else {
        s.parse::<usize>().ok()
    }
}
