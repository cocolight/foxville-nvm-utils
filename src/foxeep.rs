// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// foxeep.rs -- Intel I225 / I226 (Foxville) Shadow RAM 转储（*.eep）解析与比对工具
//
// 协议：GPL-3.0-or-later（见仓库根 LICENSE）
//
// 姊妹工具：
//     foxflash.exe  解析完整 flash 镜像（.bin / .zip / .tar.gz）
//     foxeep.exe    解析 Shadow RAM 转储（.eep，eeupdate /DUMP 的产物）
//
// BUILD（零依赖，纯 Rust 标准库）:
//     rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o foxeep.exe foxeep.rs
//
// ============================================================================
// 0. .eep 是什么（实测归纳，2026-09-28）
// ============================================================================
//
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
//     即 word 值 == .bin 里该位置的 u16 小端值。w0 = 0x60BE 就写成 "BE60"？
//     —— 不是。实测 .bin 前 6 字节 60 BE B4 02 68 0E，对应 .eep 前三 word
//     "BE60 02B4 0E68"：0xBE60 拆成字节是(小端) 60 BE，正好是 .bin 的 byte0..1。
//     所以 **.eep word i  ==  .bin 的 u16le @byte 2i**，与内核 "word addressable,
//     little-endian" 一致。
//   * 总长固定 **0x800 = 2048 word**（4 KB Shadow RAM），8 word/行 → 256 数据行。
//   * ';' 开头是注释；官方包用 ";-------Range [0x??-0x??]-----" 每 64 word 分一节。
//
// 实证：本机 60BEB402680E.eep 与同名 .bin 的 2048 个 word **零差异**，
//       两边 NVM 校验和都是 0xBABA。
//
// 偏移量的具名依据全部来自 Intel 提交的 Linux 内核（见 NVM原理与偏移依据.md）：
//     NVM_MAC_ADDR 0x0000 / NVM_COMPAT 0x0003 / NVM_VERSION 0x0005
//     NVM_PBA_OFFSET_0 0x0008(守卫 0xFAFA) / NVM_PBA_OFFSET_1 0x0009(指针)
//     NVM_SUB_DEV_ID 0x000B / NVM_SUB_VEN_ID 0x000C / NVM_DEV_ID 0x000D
//     NVM_VEN_ID 0x000E / NVM_INIT_CTRL_2 0x000F
//     NVM_ALT_MAC_ADDR_PTR 0x0037 / NVM_CHECKSUM_REG 0x003F
//     NVM_ETRACK_WORD 0x0042 / NVM_ETRACK_HIWORD 0x0043
//
// 协议：MIT
// 作者：为「倍控 G31-1338 四口机 I225-V 固件升级」项目而写

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};

const APP: &str = "foxeep";
const VERSION: &str = "1.0 (rust)";

/// 期望的 Shadow RAM 长度（word）。eeupdate /DUMP 固定产出 2048 word。
const EXPECTED_WORDS: usize = 0x800;

static JSON_MODE: AtomicBool = AtomicBool::new(false);

fn note(msg: &str) {
    if JSON_MODE.load(Ordering::Relaxed) {
        eprintln!("{}", msg);
    } else {
        println!("{}", msg);
    }
}

// ============================================================================
// 1. 平台相关 + 显示宽度（中文等全角字符在等宽终端里占 2 列）
// ============================================================================

#[cfg(windows)]
extern "system" {
    fn SetConsoleOutputCP(code_page: u32) -> i32;
    fn SetConsoleCP(code_page: u32) -> i32;
}

fn set_console_utf8() {
    #[cfg(windows)]
    unsafe {
        SetConsoleOutputCP(65001);
        SetConsoleCP(65001);
    }
}

fn line() -> String {
    "=".repeat(72)
}

fn thin_line() -> String {
    "-".repeat(72)
}

// ============================================================================
// 2. 解析：.eep（文本 word 列表）与 .bin（二进制，按 u16 小端取 word）
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Eep,
    Bin,
}

struct Src {
    name: String,
    path: String,
    kind: Kind,
    file_bytes: usize,
    text_lines: usize,
    words: Vec<u16>,
    /// 官方 .eep 里的 ";-------Range [0x00-0x3f]-----" 分节头：(声明起始 word, 实际 word 数)
    range_headers: Vec<(usize, usize, usize)>,
    bad_tokens: usize,
}

/// 单个 word token -> u16。容忍 "0x" 前缀和 1~4 位十六进制。
fn parse_word(tok: &str) -> Option<u16> {
    let t = tok.trim();
    let t = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")).unwrap_or(t);
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
// 3. 字段解码
// ============================================================================

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

/// Foxville 的版本字：次版本占 bit0..7（BCD），主版本占 bit12..15。
/// 与 igb 老布局（次版本占 bit4..11）不同，别照抄内核 NVM_MINOR_MASK 0x0FF0。
fn nvm_version_label(w: u16) -> String {
    let major = (w & 0xF000) >> 12;
    let minor_raw = w & 0x0FFF;
    if minor_raw > 0x99 {
        return "?".to_string();
    }
    let minor = (minor_raw / 16) * 10 + (minor_raw % 16);
    format!("{}.{:02}", major, minor)
}

fn devid_label(d: u16) -> String {
    match d {
        0x15F3 => "I225-V".to_string(),
        0x15F2 => "I225-LM".to_string(),
        0x15F8 => "I225-IT/K(未验证)".to_string(),
        0x125B => "I226-LM".to_string(),
        0x125C => "I226-V".to_string(),
        0x125D => "I226-IT".to_string(),
        _ => "未知".to_string(),
    }
}

fn compat_label(w: u16) -> String {
    // NVM_COMPAT 的高字节：1MB 结构 0x0D，2MB 结构 0x05，只差 bit 0x0800 (NVM_COMPAT_LOM)
    match (w >> 8) as u8 {
        0x0D => "1MB 结构".to_string(),
        0x05 => "2MB 结构".to_string(),
        _ => "未知".to_string(),
    }
}

fn imgtype_label(t: u16) -> String {
    match t {
        0x8022 => "1MB".to_string(),
        0x80A2 => "2MB".to_string(),
        _ => "未知".to_string(),
    }
}

/// EEPID 已知表（与 foxflash 的 KNOWN_EEPID 同源，这里只列常用条目）
fn known_eepid(e: u32) -> String {
    match e {
        0x80000150 => "FXVL_15F3_V_1MB_1.45（15F3 / 1MB / NVM 1.45）".to_string(),
        0x80000182 => {
            "FXVL_15F3_V_1MB_1.57（15F3 / 1MB / NVM 1.57；倍控 G31-1338 出厂）".to_string()
        }
        0x800001CE => "FXVL_15F3_V_1MB_1.68（15F3 / 1MB / NVM 1.68）".to_string(),
        0x800002FC => "FXVL_15F3_V_1MB_1.89（15F3 / 1MB / NVM 1.89）".to_string(),
        0x800003FC => {
            "Foxpond1_I225_15F3_V_1MB_1p94（15F3 / 1MB / NVM 1.94）<-- 本机待刷".to_string()
        }
        0x800002FB => "FXVL_15F2_LM_1MB_1.89（15F2 / 1MB / NVM 1.89，Vendor 0x17AA）".to_string(),
        0x800003BB => "Intel 官方 FoxPond1_I225_15F2_LM_2MB_1p94".to_string(),
        0x800003BC => "Intel 官方 Foxpond1_I225_15F2_LM_1MB_1p94".to_string(),
        0x80000290 => "FXVL_125C_V_1MB_2.14（I226-V / 1MB）".to_string(),
        0x8000028D => "FXVL_125C_V_2MB_2.14（I226-V / 2MB）".to_string(),
        0x80000425 => {
            "FXVL_125C_V_1MB_2.27 或 2.32（I226-V / 1MB；看 word 0x05 定版本）".to_string()
        }
        0x80000422 => {
            "FXVL_125C_V_2MB_2.27 或 2.32（I226-V / 2MB；看 word 0x05 定版本）".to_string()
        }
        _ => String::new(),
    }
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
    if guard != 0xFAFA {
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

struct Info {
    src: Src,
    mac: String,
    compat: u16,
    version: u16,
    imgtype: u16,
    subdev: u16,
    subven: u16,
    devid: u16,
    venid: u16,
    init2: u16,
    altmac: Option<String>,
    altmac_ptr: u16,
    checksum: u16,
    checksum_word: u16,
    etrack: u32,
    pba: String,
    pba_note: String,
    last_used: usize,
    notes: Vec<String>,
}

fn analyze(s: Src) -> Info {
    let w = &s.words;
    let mut notes: Vec<String> = Vec::new();

    if s.words.len() != EXPECTED_WORDS {
        notes.push(format!(
            "word 数 {} != 期望 {}（eeupdate /DUMP 固定产出 0x800 word）",
            s.words.len(),
            EXPECTED_WORDS
        ));
    }
    if s.bad_tokens > 0 {
        notes.push(format!("有 {} 个无法解析的 token（非 1~4 位十六进制）", s.bad_tokens));
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
    if checksum != 0xBABA {
        notes.push(format!(
            "NVM 校验和 sum(w0..0x3F)=0x{:04X} != 0xBABA（内核 NVM_SUM）",
            checksum
        ));
    }
    let venid = word(w, 0x0E);
    if venid != 0x8086 {
        notes.push(format!("VendorID w0x0E=0x{:04X} != 0x8086，可能不是 Intel NVM", venid));
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
            "EtrackID 0x{:08X} 高位不是 0x8000（内核 NVM_ETRACK_VALID），字段可能无效",
            etrack
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

// ============================================================================
// 4. 输出
// ============================================================================

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Eep => ".eep (Shadow RAM 转储)",
        Kind::Bin => ".bin (flash 镜像, 按 u16 小端取 word)",
    }
}

fn show_result(r: &Info) {
    let s = &r.src;
    println!("{}", line());
    println!("文件    : {}", s.name);
    println!("来源    : {}", s.path);
    println!(
        "类型    : {}",
        kind_label(s.kind)
    );
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
        println!("分节头  : {} 个 Range 注释（官方 .eep 格式）", s.range_headers.len());
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
        nvm_version_label(r.version)
    );
    println!(
        "镜像类型    w0x10 : 0x{:04X}  -> {}   (与 w0x03 互证；公开头文件里无名)",
        r.imgtype,
        imgtype_label(r.imgtype)
    );
    println!("{}", thin_line());
    println!("PBA 板号        : {}", if r.pba.is_empty() { "<无>" } else { &r.pba });
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
        if r.checksum == 0xBABA { "OK" } else { "不符" },
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

fn show_dump(r: &Info, start: usize, end: usize) {
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
                row.push_str(&format!("{:04X} ", word(&r.src.words, i + k)));
            } else {
                row.push_str("     ");
            }
        }
        println!("{}", row.trim_end());
        i += 8;
    }
}

fn show_compare(rs: &[Info]) {
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
        println!(
            "{}  vs  {}",
            base.src.name, other.src.name
        );
        println!(
            "  比对范围 : word 0x000 .. 0x{:03X}（{} 个 word）",
            n - 1,
            n
        );
        if diffs.is_empty() {
            println!("  结果     : 完全相同 ✅");
        } else {
            println!("  结果     : {} 个 word 不同", diffs.len());
            for i in diffs.iter().take(24) {
                println!(
                    "    w0x{:03X}   A=0x{:04X}   B=0x{:04X}",
                    i,
                    base.src.words[*i],
                    other.src.words[*i]
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

fn print_json(rs: &[Info]) {
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
        println!("    \"nvmver_label\": \"{}\",", nvm_version_label(r.version));
        println!("    \"imgtype\": \"0x{:04X}\",", r.imgtype);
        println!("    \"subdev\": \"0x{:04X}\",", r.subdev);
        println!("    \"subven\": \"0x{:04X}\",", r.subven);
        println!("    \"devid\": \"0x{:04X}\",", r.devid);
        println!("    \"devid_label\": \"{}\",", devid_label(r.devid));
        println!("    \"venid\": \"0x{:04X}\",", r.venid);
        println!("    \"init2\": \"0x{:04X}\",", r.init2);
        println!("    \"pba\": \"{}\",", json_escape(&r.pba));
        println!("    \"altmac\": \"{}\",", r.altmac.clone().unwrap_or_default());
        println!("    \"checksum\": \"0x{:04X}\",", r.checksum);
        println!("    \"checksum_ok\": {},", r.checksum == 0xBABA);
        println!("    \"eepid\": \"0x{:08X}\",", r.etrack);
        println!("    \"eepid_note\": \"{}\",", json_escape(&known_eepid(r.etrack)));
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

fn usage() {
    println!("{} {} —— Intel I225/I226 Shadow RAM 转储（.eep）解析与比对", APP, VERSION);
    println!();
    println!("用法:");
    println!("  foxeep.exe <文件.eep> [文件2.eep|镜像.bin ...]   解析，多个则逐 word 比对");
    println!("  foxeep.exe <目录> [-r]                          扫描目录（只收 .eep）");
    println!("  foxeep.exe <文件.eep> --dump 0x00-0x7f          打印原始 word");
    println!("  foxeep.exe --json <文件.eep>                    机器可读输出（stdout 纯 JSON）");
    println!("  foxeep.exe --help                               本帮助");
    println!();
    println!("说明:");
    println!("  .eep 是 eeupdate /DUMP 产出的**文本**文件，每行 8 个十六进制 word，");
    println!("  固定 0x800=2048 word（4KB Shadow RAM）；';' 开头为注释。");
    println!("  实测 .eep word i  ==  .bin 的 u16 小端 @字节 2i，故可直接与 .bin 比对。");
    println!();
    println!("  --dump 的范围写法：0x00-0x7f（十六进制）或 0-127（十进制）");
    println!("  同时给 .eep 和 .bin 时会自动按 word 对齐比对，列出所有不同的 word。");
}

// ============================================================================
// 5. 输入收集
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
                let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                if is_eep_name(&name) {
                    if let Ok(raw) = fs::read(&p) {
                        let text = String::from_utf8_lossy(&raw).to_string();
                        let src = parse_eep_text(&name, &p.to_string_lossy(), &text, raw.len());
                        out.push(src);
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
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
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

fn collect(targets: &[String], recursive: bool) -> Vec<Src> {
    let mut out = Vec::new();
    for t in targets {
        let path = PathBuf::from(t);
        if path.is_dir() {
            let (got, skipped) = collect_from_dir(&path, recursive);
            if got.is_empty() {
                note(&format!("[!] 目录里没有 .eep 文件：{}", t));
            } else if skipped > 0 {
                note(&format!(
                    "[i] {}  ->  已跳过 {} 个非 .eep 的文件",
                    t, skipped
                ));
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

/// 解析 --dump 的范围：支持 "0x00-0x7f" / "0-127" / 单个 "0x40"
fn parse_range(s: &str) -> Option<(usize, usize)> {
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

fn parse_num(s: &str) -> Option<usize> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        usize::from_str_radix(rest, 16).ok()
    } else {
        s.parse::<usize>().ok()
    }
}

// ============================================================================
// 6. 入口
// ============================================================================

fn main() {
    set_console_utf8();
    let argv: Vec<String> = env::args().skip(1).collect();

    let mut recursive = false;
    let mut json = false;
    let mut dump: Option<(usize, usize)> = None;
    let mut targets: Vec<String> = Vec::new();

    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-r" | "--recursive" | "-R" => recursive = true,
            "--json" => {
                json = true;
                JSON_MODE.store(true, Ordering::Relaxed);
            }
            "--dump" => {
                if let Some(v) = it.next() {
                    match parse_range(v) {
                        Some(r) => dump = Some(r),
                        None => {
                            eprintln!("[!] --dump 范围无法解析：{}（示例 0x00-0x7f）", v);
                            process::exit(2);
                        }
                    }
                }
            }
            "--help" | "-h" | "/?" => {
                usage();
                return;
            }
            _ => targets.push(a.clone()),
        }
    }

    if targets.is_empty() {
        usage();
        println!();
        println!("提示：Windows 下可以直接把 .eep 文件拖到 foxeep.exe 图标上运行。");
        process::exit(1);
    }

    let srcs = collect(&targets, recursive);
    if srcs.is_empty() {
        process::exit(1);
    }

    let results: Vec<Info> = srcs.into_iter().map(analyze).collect();

    if json {
        print_json(&results);
        return;
    }

    for r in &results {
        show_result(r);
        if let Some((a, b)) = dump {
            show_dump(r, a, b);
        }
    }
    show_compare(&results);

    if results.len() == 1 && dump.is_none() {
        println!();
        println!(
            "提示：再给一个 .eep 或 .bin 就能逐 word 比对（例：foxeep.exe a.eep b.bin）"
        );
    }
}
