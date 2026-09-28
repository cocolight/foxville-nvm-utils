// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// foxflash.rs -- Intel I225 / I226 (Foxville) NVM 固件镜像离线体检工具
//
// BUILD (零依赖，纯 Rust 标准库):
//     rustc -O -C target-feature=+crt-static -o foxflash.exe foxflash.rs
// 如需调试信息更小：
//     rustc -O -C strip=symbols -C target-feature=+crt-static -o foxflash.exe foxflash.rs
//
// 协议：GPL-3.0-or-later（见仓库根 LICENSE）
// 本项目源于「倍控 G31-1338 四口机 I225-V 固件升级」

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};

const APP: &str = "foxflash";
const VERSION: &str = "2.1 (rust)";

/// `--json` 开关。开启后所有 [i]/[!] 提示改走 stderr，
/// 保证 stdout 是**纯 JSON**（否则提示行会混在 JSON 前面，严格解析器直接报错）。
static JSON_MODE: AtomicBool = AtomicBool::new(false);

/// 统一的提示输出口：正常模式走 stdout，--json 模式走 stderr。
fn note(msg: &str) {
    if JSON_MODE.load(Ordering::Relaxed) {
        eprintln!("{}", msg);
    } else {
        println!("{}", msg);
    }
}

// ============================================================================
// 0. 平台相关：强制 Windows 控制台走 UTF-8，否则中文会按 GBK 输出成乱码
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

// ============================================================================
// 1. 显示宽度（中文等全角字符在等宽终端里占 2 列）
// ============================================================================

fn char_width(c: char) -> usize {
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

fn display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn pad(s: &str, width: usize) -> String {
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

fn truncate(s: &str, max: usize) -> String {
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
// 2. MD5（标准库没有，手写一份）
// ============================================================================

const MD5_S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21,
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const MD5_K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

fn md5_rotl(x: u32, n: u32) -> u32 {
    (x << n) | (x >> (32 - n))
}

struct Md5 {
    state: [u32; 4],
    buf: Vec<u8>,
    len: u64,
}

impl Md5 {
    fn new() -> Md5 {
        Md5 {
            state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476],
            buf: Vec::with_capacity(64),
            len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.len += data.len() as u64;
        self.buf.extend_from_slice(data);
        while self.buf.len() >= 64 {
            let block: Vec<u8> = self.buf.drain(..64).collect();
            Self::process(&mut self.state, &block);
        }
    }

    fn finish(mut self) -> String {
        let bit_len = (self.len * 8) as u64;
        let mut tail = Vec::new();
        tail.push(0x80u8);
        // 补零到满足 (have + 1 + zeros + 8) % 64 == 0
        let have = self.buf.len();
        let need = (56isize - (have as isize + 1)).rem_euclid(64) as usize;
        tail.extend_from_slice(&vec![0u8; need]);
        tail.extend_from_slice(&bit_len.to_le_bytes());

        let mut padded: Vec<u8> = Vec::with_capacity(have + tail.len() + 64);
        padded.extend_from_slice(&self.buf);
        padded.extend_from_slice(&tail);
        let mut i = 0usize;
        while i + 64 <= padded.len() {
            Self::process(&mut self.state, &padded[i..i + 64]);
            i += 64;
        }

        let mut out = String::new();
        for w in self.state.iter() {
            out.push_str(&format!("{:02x}{:02x}{:02x}{:02x}",
                w & 0xff, (w >> 8) & 0xff, (w >> 16) & 0xff, (w >> 24) & 0xff));
        }
        out
    }

    fn process(state: &mut [u32; 4], block: &[u8]) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (state[0], state[1], state[2], state[3]);
        for i in 0..64usize {
            let (f, g) = match i / 16 {
                0 => (((b & c) | ((!b) & d)), i),
                1 => (((d & b) | ((!d) & c)), (5 * i + 1) % 16),
                2 => ((b ^ c ^ d), (3 * i + 5) % 16),
                _ => ((c ^ (b | (!d))), (7 * i) % 16),
            };
            let tmp = a
                .wrapping_add(f)
                .wrapping_add(MD5_K[i])
                .wrapping_add(m[g]);
            let new_b = b.wrapping_add(md5_rotl(tmp, MD5_S[i]));
            a = d;
            d = c;
            c = b;
            b = new_b;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }
}

fn md5_hex(data: &[u8]) -> String {
    let mut h = Md5::new();
    h.update(data);
    h.finish()
}

// ============================================================================
// 3. DEFLATE 解压（为了能直接读 zip / tar.gz，不引入外部 crate）
// ============================================================================

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bits: u64,
    nbits: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> BitReader<'a> {
        BitReader { data, pos: 0, bits: 0, nbits: 0 }
    }

    fn need(&mut self, n: u32) -> Result<(), String> {
        while self.nbits < n {
            if self.pos >= self.data.len() {
                return Err("数据流提前结束".to_string());
            }
            self.bits |= (self.data[self.pos] as u64) << self.nbits;
            self.pos += 1;
            self.nbits += 8;
        }
        Ok(())
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = (self.bits & ((1u64 << n) - 1)) as u32;
        self.bits >>= n;
        self.nbits -= n;
        Ok(v)
    }

    fn align(&mut self) {
        let drop = self.nbits % 8;
        self.bits >>= drop;
        self.nbits -= drop;
    }
}

struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn decode(&self, br: &mut BitReader) -> Result<u16, String> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16usize {
            code |= br.bits(1)? as i32;
            let count = self.counts[len] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + code - first) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("非法 Huffman 编码".to_string())
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
// code length code 的读取顺序
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn build_huffman(lengths: &[u8]) -> Result<Huffman, String> {
    let mut counts = [0u16; 16];
    for &l in lengths.iter() {
        counts[l as usize] += 1;
    }
    counts[0] = 0;
    let mut offs = [0u16; 16];
    for len in 1..15usize {
        offs[len + 1] = offs[len] + counts[len];
    }
    let mut symbols = vec![0u16; lengths.len()];
    for (sym, &l) in lengths.iter().enumerate() {
        if l != 0 {
            symbols[offs[l as usize] as usize] = sym as u16;
            offs[l as usize] += 1;
        }
    }
    Ok(Huffman { counts, symbols })
}

fn inflate(data: &[u8], cap: usize) -> Result<Vec<u8>, String> {
    let mut br = BitReader::new(data);
    let mut out: Vec<u8> = Vec::new();

    loop {
        let last = br.bits(1)?;
        let btype = br.bits(2)?;

        match btype {
            0 => {
                // 未压缩块
                br.align();
                let len = br.bits(16)? as usize;
                let _nlen = br.bits(16)?;
                for _ in 0..len {
                    if br.pos >= br.data.len() {
                        return Err("stored 块数据不足".to_string());
                    }
                    out.push(br.data[br.pos]);
                    br.pos += 1;
                }
            }
            1 | 2 => {
                let (lit, dist) = if btype == 1 {
                    let mut l = vec![8u8; 288];
                    for i in 144..256 {
                        l[i] = 9;
                    }
                    for i in 256..280 {
                        l[i] = 7;
                    }
                    for i in 280..288 {
                        l[i] = 8;
                    }
                    let d = vec![5u8; 30];
                    (l, d)
                } else {
                    let hlit = br.bits(5)? as usize + 257;
                    let hdist = br.bits(5)? as usize + 1;
                    let hclen = br.bits(4)? as usize + 4;
                    let mut cl = [0u8; 19];
                    for i in 0..hclen {
                        cl[CL_ORDER[i]] = br.bits(3)? as u8;
                    }
                    let clh = build_huffman(&cl)?;
                    let mut lengths = Vec::with_capacity(hlit + hdist);
                    let total = hlit + hdist;
                    while lengths.len() < total {
                        let sym = clh.decode(&mut br)? as usize;
                        match sym {
                            0..=15 => lengths.push(sym as u8),
                            16 => {
                                if lengths.is_empty() {
                                    return Err("Huffman 重复码缺少前值".to_string());
                                }
                                let prev = *lengths.last().unwrap();
                                let rep = 3 + br.bits(2)? as usize;
                                for _ in 0..rep {
                                    lengths.push(prev);
                                }
                            }
                            17 => {
                                let rep = 3 + br.bits(3)? as usize;
                                for _ in 0..rep {
                                    lengths.push(0);
                                }
                            }
                            18 => {
                                let rep = 11 + br.bits(7)? as usize;
                                for _ in 0..rep {
                                    lengths.push(0);
                                }
                            }
                            _ => return Err("非法 code length symbol".to_string()),
                        }
                    }
                    if lengths.len() > total {
                        lengths.truncate(total);
                    }
                    let dl = lengths[hlit..].to_vec();
                    let ll = lengths[..hlit].to_vec();
                    (ll, dl)
                };

                let lh = build_huffman(&lit)?;
                let dh = build_huffman(&dist)?;

                loop {
                    let sym = lh.decode(&mut br)? as usize;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let idx = sym - 257;
                        if idx >= 29 {
                            return Err("非法长度符号".to_string());
                        }
                        let len = LEN_BASE[idx] as usize + br.bits(LEN_EXTRA[idx] as u32)? as usize;
                        let dsym = dh.decode(&mut br)? as usize;
                        if dsym >= 30 {
                            return Err("非法距离符号".to_string());
                        }
                        let dist = DIST_BASE[dsym] as usize
                            + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
                        if dist > out.len() {
                            return Err("距离超出已输出窗口".to_string());
                        }
                        let start = out.len() - dist;
                        for k in 0..len {
                            let b = out[start + k];
                            out.push(b);
                        }
                    }
                    if out.len() > cap {
                        return Err("解压结果超出上限".to_string());
                    }
                }
            }
            _ => return Err("不支持的 deflate 块类型 (BTYPE=3)".to_string()),
        }

        if out.len() > cap {
            return Err("解压结果超出上限".to_string());
        }
        if last == 1 {
            break;
        }
    }
    Ok(out)
}

// gzip：只处理单成员
fn gunzip(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 18 || data[0] != 0x1f || data[1] != 0x8b {
        return Err("不是 gzip 数据".to_string());
    }
    if data[2] != 8 {
        return Err("gzip 压缩方法不是 deflate".to_string());
    }
    let flg = data[3];
    let mut p: usize = 10;
    if flg & 0x04 != 0 {
        // FEXTRA
        if p + 2 > data.len() {
            return Err("gzip 头损坏".to_string());
        }
        let xlen = u16::from_le_bytes([data[p], data[p + 1]]) as usize;
        p += 2 + xlen;
    }
    for &mask in [0x08u8, 0x10, 0x02].iter() {
        if flg & mask != 0 {
            while p < data.len() && data[p] != 0 {
                p += 1;
            }
            p += 1;
        }
    }
    if p >= data.len() {
        return Err("gzip 头损坏".to_string());
    }
    inflate(&data[p..], 256 * 1024 * 1024)
}

// ============================================================================
// 4. 压缩包读取：zip / tar.gz（够用的最小实现，不支持 zip64 / 加密）
// ============================================================================

fn u32le_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn u16le_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

struct Entry {
    name: String,
    source: String,
    data: Vec<u8>,
}

fn read_zip(path: &str, data: &[u8]) -> Vec<Entry> {
    let mut out = Vec::new();
    // 找 End Of Central Directory
    let search_from = data.len().saturating_sub(66 * 1024);
    let mut eocd: Option<usize> = None;
    let mut i = data.len() - 4;
    while i >= search_from {
        if u32le_at(data, i) == 0x06054b50 {
            eocd = Some(i);
            break;
        }
        i = i.saturating_sub(1);
        if i == 0 {
            break;
        }
    }
    let eocd = match eocd {
        Some(v) => v,
        None => return out,
    };
    let count = u16le_at(data, eocd + 10) as usize;
    let mut off = u32le_at(data, eocd + 16) as usize;

    for _ in 0..count {
        if off + 46 > data.len() || u32le_at(data, off) != 0x02014b50 {
            break;
        }
        let method = u16le_at(data, off + 10);
        let comp_size = u32le_at(data, off + 20) as usize;
        let raw_name_len = u16le_at(data, off + 28) as usize;
        let extra_len = u16le_at(data, off + 30) as usize;
        let comment_len = u16le_at(data, off + 32) as usize;
        let lho = u32le_at(data, off + 42) as usize;
        let name = String::from_utf8_lossy(&data[off + 46..off + 46 + raw_name_len]).to_string();
        off += 46 + raw_name_len + extra_len + comment_len;

        let base = name.rsplit('/').next().unwrap_or(&name).to_string();
        if !is_image_name(&base) {
            continue;
        }
        if lho + 30 > data.len() || u32le_at(data, lho) != 0x04034b50 {
            continue;
        }
        let lname_len = u16le_at(data, lho + 26) as usize;
        let lextra_len = u16le_at(data, lho + 28) as usize;
        let dstart = lho + 30 + lname_len + lextra_len;
        let dend = (dstart + comp_size).min(data.len());
        let raw = &data[dstart..dend];

        let payload = match method {
            0 => Ok(raw.to_vec()),
            8 => inflate(raw, 256 * 1024 * 1024),
            _ => Err(format!("不支持的压缩方法 {}", method)),
        };
        match payload {
            Ok(bytes) => out.push(Entry {
                name: base,
                source: format!("{}::{}", path, name),
                data: bytes,
            }),
            Err(e) => println!("[!] 跳过 {} ({})", base, e),
        }
    }
    out
}

fn read_tar(path: &str, data: &[u8]) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p + 512 <= data.len() {
        let name_bytes = &data[p..p + 100];
        if name_bytes[0] == 0 {
            break;
        }
        let name = String::from_utf8_lossy(name_bytes)
            .trim_matches(|c| c == '\0' || c == ' ')
            .to_string();
        let typeflag = data[p + 156];
        let size_str = String::from_utf8_lossy(&data[p + 124..p + 136])
            .trim_matches(|c| c == '\0' || c == ' ')
            .to_string();
        let size: usize = size_str.parse().unwrap_or(0);
        let dstart = p + 512;
        let dend = (dstart + size).min(data.len());
        let base = name.rsplit('/').next().unwrap_or(&name).to_string();

        if typeflag == b'0' || typeflag == 0 {
            if is_image_name(&base) {
                out.push(Entry {
                    name: base,
                    source: format!("{}::{}", path, name),
                    data: data[dstart..dend].to_vec(),
                });
            }
        }
        let blocks = (size + 511) / 512;
        p = dstart + blocks * 512;
    }
    out
}

fn is_image_name(n: &str) -> bool {
    let lower = n.to_ascii_lowercase();
    lower.ends_with(".bin") || lower.ends_with(".img") || lower.ends_with(".eep")
        || lower.ends_with(".dat")
}

/// 目录扫描专用：只认 .bin。
///
/// 同一个目录里常常混着 .eep（shadow RAM dump，10496 字节，布局跟完整 flash
/// 镜像完全不同）、.txt、.cfg、.md。这些东西一旦被当成 flash 镜像解析，
/// 读出来的 0x07 / 0x20 / 0x84 全是错的，还会被拉进末尾的汇总对照表，
/// 把真正该比对的 .bin 淹没掉。所以扫描目录时一律跳过，只留 .bin。
/// （显式在命令行上单独指定某个文件时不受此限制，照样解析并给提示。）
fn is_bin_name(n: &str) -> bool {
    n.to_ascii_lowercase().ends_with(".bin")
}

// ============================================================================
// 5. 核心：解析 Foxville NVM 镜像头
// ============================================================================

// ============================================================================
// 偏移量的一手依据（2026-09-28 补，来源：Linux 内核源码，Intel 提交、GPL 授权）
// ----------------------------------------------------------------------------
// 关键前提：.bin 是 NVM「字空间」按字节展开的镜像，word N <-> byte 2N。
//   证据 1  drivers/net/ethernet/intel/igc/igc_ethtool.c
//           igc_ethtool_get_eeprom_len() 返回 hw->nvm.word_size * 2
//           first_word = eeprom->offset >> 1
//           注释原文："Device's eeprom is always little-endian, word addressable"
//   证据 2  igc_nvm.c: igc_validate_nvm_checksum() 对 word 0..NVM_CHECKSUM_REG
//           求和并要求等于 NVM_SUM(0xBABA)。实测 35 个镜像 31 个精确符合。
//   证据 3  官方 Foxpond_Map_File_v01.txt 与内核 NVM_ALT_MAC_ADDR_PTR(0x0037)
//           同时指向 Alternate MAC，实测 word0x37=0x0119 -> byte 0x232 是 MAC。
//
// drivers/net/ethernet/intel/igb/e1000_defines.h（I210/I350 共用字表，实测对
// Foxville 同样成立）里的具名字段：
//   NVM_MAC_ADDR          0x0000  -> byte 0x00  MAC 地址
//   NVM_COMPAT            0x0003  -> byte 0x06  兼容性字（高字节 bit0x08 见下）
//   NVM_VERSION           0x0005  -> byte 0x0A  NVM 版本字
//   NVM_PBA_OFFSET_0/1    8/9     -> byte 0x10  PBA 指针，守卫值 NVM_PBA_PTR_GUARD 0xFAFA
//   NVM_SUB_DEV_ID        0x000B  -> byte 0x16
//   NVM_SUB_VEN_ID        0x000C  -> byte 0x18
//   NVM_DEV_ID            0x000D  -> byte 0x1A  Device ID
//   NVM_VEN_ID            0x000E  -> byte 0x1C  Vendor ID
//   NVM_INIT_CTRL_2       0x000F  -> byte 0x1E
//   NVM_ALT_MAC_ADDR_PTR  0x0037  -> byte 0x6E  Alternate MAC 的「字指针」
//   NVM_CHECKSUM_REG      0x003F  -> byte 0x7E  校验和字
//   NVM_ETRACK_WORD       0x0042  -> byte 0x84  EtrackID 低 16 位
//   NVM_ETRACK_HIWORD     0x0043  -> byte 0x86  EtrackID 高 16 位（有效标志 0x8000）
//   NVM_SUM               0xBABA  校验和目标值
//
// 注意（必须纠正的旧说法）：
//   * byte 0x07 不是「闪存容量索引」。它是 NVM_COMPAT 字的高字节；1MB(0x0D20)
//     与 2MB(0x0520) 只差 bit 0x0800，即 e1000e 里定义的 NVM_COMPAT_LOM
//     （LAN On Motherboard）。它跟容量 100% 相关（35/35），但语义是 LOM 位。
//   * byte 0x20（word 0x10）在公开头文件里查不到名字，仍是归纳结论。
//   * byte 0x232 不是「备用/另一口 MAC」，而是 Alternate MAC Address。
// ============================================================================

struct ImageInfo {
    name: String,
    source: String,
    size: usize,
    md5: String,
    ok: bool,
    flash_idx: u8,
    flash_label: String,
    imgtype: u16,
    imgtype_label: String,
    nvmver: u16,
    nvmver_label: String,
    mac: String,
    vendor: u16,
    devid: u16,
    devid_label: String,
    subvendor: u16,
    subdevice: u16,
    eepid: u32,
    eepid_note: String,
    /// word 0..0x3F 求和（含 word 0x3F 校验和字本身）
    checksum: u16,
    /// 是否等于内核定义的 NVM_SUM(0xBABA)
    checksum_ok: bool,
    /// word 0x37 = Alternate MAC 的字指针；0xFFFF 表示该区块已移除
    altmac_ptr: u16,
    altmac: String,
    notes: Vec<String>,
    data: Vec<u8>,
}

fn flash_idx_label(b: u8) -> String {
    match b {
        0x0D => "1MB".to_string(),
        0x05 => "2MB".to_string(),
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

fn devid_label(d: u16) -> String {
    match d {
        0x15F3 => "I225-V".to_string(),
        0x15F2 => "I225-LM".to_string(),
        0x15F8 => "I225-IT(未验证)".to_string(),
        0x125B => "I226-LM".to_string(),
        0x125C => "I226-V".to_string(),
        0x125D => "I226-IT".to_string(),
        _ => "未知".to_string(),
    }
}

/// 已知 EEPID 对照表
///
/// 标定来源两处，互相印证：
///  1) Intel 官方驱动包 Release_31.2.2：镜像文件名自带 EEPID 后缀
///     （FoxPond1_I225_15F2_2MB_1p94_800003BB.bin），nvmupdate.cfg 里写的也是同一批值；
///  2) 第三方仓库 cocolight/Intel-I226-V-NVM-Firmware 的 README 表格，
///     20 个 I225/I226 镜像的 EtrackID 与实测 @0x84 逐条命中（2026-09-28 实测）。
///
/// *注意*：EEPID 不能单独当版本号用。I226 的 1MB 2.27 与 2.32 共用 0x80000425，
/// 2MB 2.27 与 2.32 共用 0x80000422。要定版本，以 0x0A 版本字为准。
const KNOWN_EEPID: &[(u32, &str)] = &[
    // ---- I225-V (15F3) 1MB ----
    (0x80000150, "FXVL_15F3_V_1MB_1.45（15F3 / 1MB / NVM 1.45）"),
    (0x80000182, "FXVL_15F3_V_1MB_1.57（15F3 / 1MB / NVM 1.57；倍控 G31-1338 出厂就是这个）"),
    (0x800001CE, "FXVL_15F3_V_1MB_1.68（15F3 / 1MB / NVM 1.68）"),
    (0x800002FC, "FXVL_15F3_V_1MB_1.89（15F3 / 1MB / NVM 1.89）"),
    (0x800003FC, "Foxpond1_I225_15F3_V_1MB_1p94（15F3 / 1MB / NVM 1.94）<-- 本机待刷"),
    // ---- I225-V (15F3) 2MB ----
    (0x8000014B, "FXVL_15F3_V_2MB_1.45（15F3 / 2MB / NVM 1.45）"),
    (0x80000185, "FXVL_15F3_V_2MB_1.57（15F3 / 2MB / NVM 1.57）"),
    (0x800001C7, "FXVL_15F3_V_2MB_1.68（15F3 / 2MB / NVM 1.68）"),
    (0x800002F4, "FXVL_15F3_V_2MB_1.89（15F3 / 2MB / NVM 1.89）"),
    // ---- I225-LM (15F2) ----
    (0x800002FB, "FXVL_15F2_LM_1MB_1.89（15F2 / 1MB / NVM 1.89，Vendor 0x17AA）"),
    (0x800003BB, "Intel 官方 FoxPond1_I225_15F2_LM_2MB_1p94（15F2 / 2MB / NVM 1.94）"),
    (0x800003BC, "Intel 官方 Foxpond1_I225_15F2_LM_1MB_1p94（15F2 / 1MB / NVM 1.94）"),
    // ---- I226-V (125C) 1MB ----
    (0x80000290, "FXVL_125C_V_1MB_2.14（I226-V / 1MB / NVM 2.14）"),
    (0x80000308, "FXVL_125C_V_1MB_2.17（I226-V / 1MB / NVM 2.17）"),
    (0x8000039D, "FXVL_125C_V_1MB_2.23（I226-V / 1MB / NVM 2.23）"),
    (0x80000425, "FXVL_125C_V_1MB_2.27 或 2.32（I226-V / 1MB；两版本共用此 ID，看 0x0A 定版本）"),
    // ---- I226-V (125C) 2MB ----
    (0x8000028D, "FXVL_125C_V_2MB_2.14（I226-V / 2MB / NVM 2.14）"),
    (0x80000303, "FXVL_125C_V_2MB_2.17（I226-V / 2MB / NVM 2.17）"),
    (0x80000371, "FXVL_125C_V_2MB_2.22（I226-V / 2MB / NVM 2.22）"),
    (0x800003AD, "FXVL_125C_V_2MB_2.25（I226-V / 2MB / NVM 2.25）"),
    (0x80000422, "FXVL_125C_V_2MB_2.27 或 2.32（I226-V / 2MB；两版本共用此 ID，看 0x0A 定版本）"),
    // ---- I226-LM (125B) / I226-IT (125D) ----
    (0x80000424, "FXVL_125B_LM_1MB_2.32（I226-LM / 1MB / NVM 2.32）"),
    (0x80000421, "FXVL_125B_LM_2MB_2.32（I226-LM / 2MB / NVM 2.32）"),
    (0x80000433, "FXVL_125D_IT_1MB_2.32（I226-IT / 1MB / NVM 2.32）"),
    (0x80000431, "FXVL_125D_IT_2MB_2.32（I226-IT / 2MB / NVM 2.32）"),
];

/// 查表。这张表是唯一数据源，--list-known 也走它，不会出现"改了表忘了改帮助"。
fn known_eepid(e: u32) -> String {
    KNOWN_EEPID
        .iter()
        .find(|(k, _)| *k == e)
        .map(|(_, v)| v.to_string())
        .unwrap_or_default()
}

/// 版本字解码 —— 完全照抄内核 igb/e1000_82575.c 里 igb_get_fw_version() 的算法。
///
/// 内核常量（igb/e1000_defines.h）：
///     NVM_MAJOR_MASK 0xF000   NVM_MAJOR_SHIFT 12
///     NVM_MINOR_MASK 0x0FF0   NVM_MINOR_SHIFT 4
///     NVM_HEX_CONV   16       NVM_HEX_TENS   10
///
/// 内核伪码（igb/e1000_82575.c: igb_get_fw_version）：
///     major = (w & NVM_MAJOR_MASK) >> NVM_MAJOR_SHIFT
///     minor = (w & NVM_MINOR_MASK) >> NVM_MINOR_SHIFT
///     minor = (minor / 16) * 10 + (minor % 16)     // NVM_HEX_CONV/TENS 折算
///
/// ★ 但 Foxville(I225/I226) 的打包方式和 igb 那一代**不一样**，不能照抄掩码：
///     igb 老布局：word = (major << 12) | (BCD(minor) << 4) | image_id
///                 例 1.94 -> 0x1940   （次版本占 bit4..11，最低 4 位是 image id）
///     Foxville  ：word = (major << 12) | (0 << 8)   | BCD(minor)
///                 例 1.94 -> 0x1094   （次版本占 bit0..7，bit8..11 恒为 0）
///   实测 35 个镜像 bit8..11 全是 0，且按低字节 BCD 解码与文件名版本 100% 吻合。
///   所以这里掩码用 0x0FFF 而不是内核的 0x0FF0 —— 差一个半字节就是"1.94"变"1.09"。
///   （major 用内核的 NVM_MAJOR_MASK 0xF000 是对的：0x1094->1，0x2032->2。）
///
/// 演算：0x1094 -> major=1，minor=0x94(148) -> 148/16=9 -> 9*10 + 4 = 94 -> "1.94"
///       0x2032 -> major=2，minor=0x32(50)  -> 50/16 =3 -> 3*10 + 2 = 32 -> "2.32"
///
/// 这解释了为什么看起来是 BCD：内核就是按"先取整十位的十六进制值再乘以 10"折的。
/// 早期版本把主版本写死成 1（只认 hi 字节 0x10），I226 全系会显示 "?"，已修。
fn nvm_version_label(w: u16) -> String {
    let major = (w & 0xF000) >> 12;
    // 注意是 0x0FFF 不是内核的 0x0FF0，原因见上面注释
    let minor_raw = w & 0x0FFF;
    if minor_raw > 0x99 {
        return "?".to_string();
    }
    // 内核的 HEX->DEC 折算（NVM_HEX_CONV 16 / NVM_HEX_TENS 10）
    let minor = (minor_raw / 16) * 10 + (minor_raw % 16);
    format!("{}.{:02}", major, minor)
}

fn parse_image(name: &str, source: &str, data: Vec<u8>) -> ImageInfo {
    let mut r = ImageInfo {
        name: name.to_string(),
        source: source.to_string(),
        size: data.len(),
        md5: md5_hex(&data),
        ok: true,
        flash_idx: 0,
        flash_label: String::new(),
        imgtype: 0,
        imgtype_label: String::new(),
        nvmver: 0,
        nvmver_label: String::new(),
        mac: String::new(),
        vendor: 0,
        devid: 0,
        devid_label: String::new(),
        subvendor: 0,
        subdevice: 0,
        eepid: 0,
        eepid_note: String::new(),
        checksum: 0,
        checksum_ok: false,
        altmac_ptr: 0,
        altmac: String::new(),
        notes: Vec::new(),
        data,
    };

    if r.data.len() < 0x88 {
        r.ok = false;
        r.notes
            .push("文件太小（不足 0x88 字节），不像 Foxville NVM 镜像".to_string());
        return r;
    }

    let d = &r.data;
    r.flash_idx = d[0x07];
    r.flash_label = flash_idx_label(d[0x07]);
    r.imgtype = u16le_at(d, 0x20);
    r.imgtype_label = imgtype_label(r.imgtype);
    r.nvmver = u16le_at(d, 0x0A);
    r.nvmver_label = nvm_version_label(r.nvmver);
    r.mac = (0..6)
        .map(|i| format!("{:02X}", d[i]))
        .collect::<Vec<String>>()
        .join(":");
    // 位置按内核 e1000_defines.h 的具名常量修正（2026-09-28）：
    //   byte 0x16 = NVM_SUB_DEV_ID (word 0x0B)
    //   byte 0x18 = NVM_SUB_VEN_ID (word 0x0C)
    //   byte 0x1A = NVM_DEV_ID     (word 0x0D)
    //   byte 0x1C = NVM_VEN_ID     (word 0x0E)
    //   byte 0x1E = NVM_INIT_CTRL_2(word 0x0F)，不是 SubDevice
    // 旧版把 0x18 当 VenID、0x1C/0x1E 当 Subsystem，在 Intel 公版镜像上
    // 因为两者都是 0x8086 而没暴露问题，遇到 OEM 镜像（如 0x17AA）就会读反。
    r.subdevice = u16le_at(d, 0x16);
    r.subvendor = u16le_at(d, 0x18);
    r.devid = u16le_at(d, 0x1A);
    r.devid_label = devid_label(r.devid);
    r.vendor = u16le_at(d, 0x1C);
    r.eepid = u32le_at(d, 0x84);
    r.eepid_note = known_eepid(r.eepid);

    // ---- NVM 校验和：内核 igc_validate_nvm_checksum() 的算法 ----
    // 对 word 0 ..= NVM_CHECKSUM_REG(0x3F) 求和，结果应为 NVM_SUM(0xBABA)。
    // 注意地址换算：word N 在 byte 2N。
    if r.data.len() >= 0x80 {
        let mut sum: u32 = 0;
        for w in 0..=0x3Fu16 {
            let off = (w as usize) * 2;
            sum += u16le_at(&r.data, off) as u32;
        }
        r.checksum = (sum & 0xFFFF) as u16;
        r.checksum_ok = r.checksum == 0xBABA;
    }

    // ---- Alternate MAC：word 0x37 是字指针，指向的 byte 偏移 = 指针 * 2 ----
    // 官方 Foxpond_Map_File_v01.txt 与内核 NVM_ALT_MAC_ADDR_PTR 都指向 0x37。
    // 0xFFFF = 该区块已移除（I226 2.13 release notes 明确写过这句话）。
    r.altmac_ptr = u16le_at(&r.data, 0x6E);
    if r.altmac_ptr != 0xFFFF {
        let off = (r.altmac_ptr as usize) * 2;
        if off + 6 <= r.data.len() {
            r.altmac = (0..6)
                .map(|i| format!("{:02X}", r.data[off + i]))
                .collect::<Vec<String>>()
                .join(":");
        }
    }

    let d = &r.data;
    if d[0x07] != 0x0D && d[0x07] != 0x05 {
        r.notes.push(format!(
            "0x07=0x{:02X} 不在已知容量索引表里，确认是不是 Foxville 镜像",
            d[0x07]
        ));
    }
    if r.imgtype != 0x8022 && r.imgtype != 0x80A2 {
        r.notes.push(format!(
            "0x20=0x{:04X} 不是已知镜像类型（1MB=0x8022 / 2MB=0x80A2）",
            r.imgtype
        ));
    }
    if r.data.len() >= 0x80 && !r.checksum_ok {
        r.notes.push(format!(
            "NVM 校验和 0x{:04X} != 0xBABA（差 0x{:04X}）。word 0..0x3F 求和应等于 \
             内核定义的 NVM_SUM；不符说明镜像被改动过，或 Intel 该批镜像会在刷写时重算校验和",
            r.checksum,
            (0xBABAu32.wrapping_sub(r.checksum as u32)) & 0xFFFF
        ));
    }

    if flash_idx_label(d[0x07]) != "未知"
        && imgtype_label(r.imgtype) != "未知"
        && flash_idx_label(d[0x07]) != imgtype_label(r.imgtype)
    {
        r.notes
            .push("0x07 与 0x20 指向的容量不一致，头部可能损坏".to_string());
    }
    if !matches!(r.vendor, 0x8086 | 0x17AA | 0x1028 | 0x8087) {
        r.notes.push(format!(
            "0x18 Vendor=0x{:04X} 非 0x8086，可能不是 Intel NVM 镜像",
            r.vendor
        ));
    }
    if !matches!(r.devid, 0x15F3 | 0x15F2 | 0x15F8 | 0x125B | 0x125C | 0x125D) {
        r.notes.push(format!(
            "0x1A DeviceID=0x{:04X} 不是已知的 Foxville ID（15F3/15F2/15F8/125B/125C/125D）",
            r.devid
        ));
    }

    // 2MB dump 的真相：是不是同一份 1MB 被写了两遍
    if r.data.len() == 2 * 1024 * 1024 {
        if r.data[..0x100000] == r.data[0x100000..] {
            r.notes.push(
                "前 1MB 与后 1MB 逐字节完全相同 -> 这是同一份 1MB 镜像被 dump 了两遍，\
                 不是四口各占一片；刷机请选 1MB 镜像"
                    .to_string(),
            );
        } else {
            r.notes
                .push("前 1MB 与后 1MB 内容不同，确为真实的 2MB 结构".to_string());
        }
    }

    r
}

// ============================================================================
// 6. 输入收集
// ============================================================================

/// 返回 (收集到的 .bin, 跳过的其它文件数)
fn collect_from_dir(dir: &Path, recursive: bool) -> (Vec<Entry>, usize) {
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
                if is_bin_name(&name) {
                    if let Ok(data) = fs::read(&p) {
                        out.push(Entry {
                            name,
                            source: p.to_string_lossy().to_string(),
                            data,
                        });
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

fn collect(targets: &[String], recursive: bool) -> Vec<Entry> {
    let mut out = Vec::new();
    for t in targets {
        let path = PathBuf::from(t);
        if path.is_dir() {
            let (got, skipped) = collect_from_dir(&path, recursive);
            if got.is_empty() {
                note(&format!("[!] 目录里没有 .bin 文件：{}", t));
            } else if skipped > 0 {
                note(&format!(
                    "[i] {}  ->  已跳过 {} 个非 .bin 的文件（.eep/.txt/.cfg/.md 不参与解析与比较）",
                    t, skipped
                ));
            }
            out.extend(got);
        } else if path.is_file() {
            let Ok(raw) = fs::read(&path) else {
                note(&format!("[!] 读取失败：{}", t));
                continue;
            };
            let s = t.to_ascii_lowercase();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if s.ends_with(".zip") {
                let got = read_zip(t, &raw);
                if got.is_empty() {
                    note(&format!("[!] zip 里没找到镜像文件：{}", t));
                }
                out.extend(got);
            } else if s.ends_with(".tar.gz") || s.ends_with(".tgz") || s.ends_with(".gz") {
                match gunzip(&raw) {
                    Ok(plain) => {
                        let got = read_tar(t, &plain);
                        if got.is_empty() {
                            note(&format!("[!] tar 里没找到镜像文件：{}", t));
                        }
                        out.extend(got);
                    }
                    Err(e) => note(&format!("[!] gzip 解压失败 {} ({})", name, e)),
                }
            } else if s.ends_with(".tar") {
                out.extend(read_tar(t, &raw));
            } else {
                if !is_bin_name(&name) {
                    note(&format!(
                        "[i] {} 不是 .bin，仍按镜像头解析，字段可能不准",
                        name
                    ));
                }
                out.push(Entry {
                    name,
                    source: t.clone(),
                    data: raw,
                });
            }
        } else {
            note(&format!("[!] 路径不存在：{}", t));
        }
    }
    out
}

// ============================================================================
// 7. 输出
// ============================================================================

fn line() -> String {
    "=".repeat(72)
}

fn thin_line() -> String {
    "-".repeat(72)
}

fn show_result(r: &ImageInfo) {
    println!("{}", line());
    println!("文件    : {}", r.name);
    if r.source != r.name {
        println!("来源    : {}", r.source);
    }
    println!("大小    : {} 字节 ({})", r.size, human_size(r.size));
    println!("MD5     : {}", r.md5);
    if !r.ok {
        for n in &r.notes {
            println!("[x] {}", n);
        }
        return;
    }
    println!("{}", thin_line());
    println!("MAC 地址    @0x00 : {}", r.mac);
    println!(
        "Compat字节  @0x07 : 0x{:02X}  -> {}   (NVM_COMPAT 高字节, bit0x08=LOM)",
        r.flash_idx, r.flash_label
    );
    println!(
        "NVM 版本    @0x0A : 0x{:04X}  -> {}",
        r.nvmver, r.nvmver_label
    );
    println!(
        "SubDev ID   @0x16 : 0x{:04X}   SubVen ID @0x18 : 0x{:04X}",
        r.subdevice, r.subvendor
    );
    println!(
        "Device ID   @0x1A : 0x{:04X}  -> {}   Ven ID @0x1C : 0x{:04X}",
        r.devid, r.devid_label, r.vendor
    );
    println!(
        "镜像类型    @0x20 : 0x{:04X}  -> {}",
        r.imgtype, r.imgtype_label
    );
    println!("EEPID/Etrack@0x84 : 0x{:08X}", r.eepid);
    if !r.eepid_note.is_empty() {
        println!("             已知  : {}", r.eepid_note);
    }
    println!(
        "NVM 校验和  w0..3F: 0x{:04X}  -> {}   (内核 NVM_SUM 应=0xBABA)",
        r.checksum,
        if r.checksum_ok { "OK" } else { "不符" }
    );
    if r.altmac_ptr == 0xFFFF {
        println!("AlternateMAC      : <该区块已移除, 指针=0xFFFF>");
    } else if !r.altmac.is_empty() {
        println!(
            "AlternateMAC@0x{:03X}: {}   (经 word 0x37 指针 0x{:04X} 定位)",
            (r.altmac_ptr as usize) * 2,
            r.altmac,
            r.altmac_ptr
        );
    }
    if !r.notes.is_empty() {
        println!("{}", thin_line());
        for n in &r.notes {
            println!("[!] {}", n);
        }
    }
}

fn human_size(n: usize) -> String {
    match n {
        1048576 => "1MB".to_string(),
        2097152 => "2MB".to_string(),
        _ => format!("{:.2} MB", n as f64 / 1048576.0),
    }
}

fn show_compare(rs: &[ImageInfo]) {
    let ok: Vec<&ImageInfo> = rs.iter().filter(|r| r.ok).collect();
    if ok.len() < 2 {
        return;
    }
    println!();
    println!("{}", line());
    println!("汇总对照");
    println!("{}", line());

    let heads: Vec<String> = ok.iter().map(|r| truncate(&r.name, 26)).collect();
    let mut width = 14usize;
    for h in &heads {
        width = width.max(display_width(h) + 2);
    }
    let mut header = pad("", 12);
    for h in &heads {
        header.push_str(&pad(h, width));
    }
    println!("{}", header);

    type Getter = fn(&ImageInfo) -> String;
    let rows: Vec<(&str, Getter)> = vec![
        ("大小", |r| format!("{} ({})", r.size, human_size(r.size))),
        ("MAC", |r| r.mac.clone()),
        ("闪存 0x07", |r| format!("0x{:02X} {}", r.flash_idx, r.flash_label)),
        ("类型 0x20", |r| format!("0x{:04X} {}", r.imgtype, r.imgtype_label)),
        ("NVM 0x0A", |r| format!("0x{:04X} {}", r.nvmver, r.nvmver_label)),
        ("DevID 0x1A", |r| format!("0x{:04X}", r.devid)),
        ("EEPID 0x84", |r| format!("0x{:08X}", r.eepid)),
    ];

    for (label, getter) in rows {
        let vals: Vec<String> = ok.iter().map(|r| getter(r)).collect();
        let same = vals.windows(2).all(|w| w[0] == w[1]);
        let mut row = pad(label, 12);
        for v in &vals {
            row.push_str(&pad(v, width));
        }
        row.push_str(if same { "  <-- 一致" } else { "  <-- 不同" });
        println!("{}", row);
    }

    if ok.len() == 2 {
        let (a, b) = (ok[0], ok[1]);
        println!();
        println!("差异摘要: {}  vs  {}", a.name, b.name);
        let fields: Vec<(&str, usize, usize)> = vec![
            ("0x07 闪存索引", 0x07, 1),
            ("0x0A NVM版本", 0x0A, 2),
            ("0x18 Vendor", 0x18, 2),
            ("0x1A DeviceID", 0x1A, 2),
            ("0x1C SubVendor", 0x1C, 2),
            ("0x1E SubDevice", 0x1E, 2),
            ("0x20 镜像类型", 0x20, 2),
            ("0x84 EEPID", 0x84, 4),
        ];
        for (label, off, size) in fields {
            if off + size > a.data.len() || off + size > b.data.len() {
                continue;
            }
            let va = &a.data[off..off + size];
            let vb = &b.data[off..off + size];
            if va != vb {
                let (sa, sb) = match size {
                    1 => (format!("0x{:02X}", va[0]), format!("0x{:02X}", vb[0])),
                    2 => (
                        format!("0x{:04X}", u16le_at(va, 0)),
                        format!("0x{:04X}", u16le_at(vb, 0)),
                    ),
                    _ => (
                        format!("0x{:08X}", u32le_at(va, 0)),
                        format!("0x{:08X}", u32le_at(vb, 0)),
                    ),
                };
                println!("   {} {} -> {}", pad(label, 14), sa, sb);
            }
        }
        if a.size == b.size {
            let n = a
                .data
                .iter()
                .zip(b.data.iter())
                .filter(|(x, y)| x != y)
                .count();
            println!(
                "   整片不同字节数: {} / {}  ({:.2}%)",
                n,
                a.size,
                100.0 * n as f64 / a.size as f64
            );
        } else {
            println!(
                "   两个文件大小不同（{} vs {}），跳过整片比对",
                a.size, b.size
            );
        }
    }
}

fn show_known() {
    println!("已记录的 EEPID / EtrackID 对照表（与镜像解析共用源码里同一份 KNOWN_EEPID 表）：");
    println!("{}", thin_line());
    for &(e, note) in KNOWN_EEPID {
        println!("   0x{:08X}   {}", e, note);
    }
}

fn usage() {
    println!("{} {}", APP, VERSION);
    println!();
    println!("用法:");
    println!("  foxflash.exe <镜像.bin> [镜像2.bin ...]    查看单个或多个镜像");
    println!("  foxflash.exe <目录> [-r]                   扫描目录（加 -r 递归）");
    println!("  foxflash.exe <包.zip> / <包.tar.gz>        不解压，直接读包内镜像");
    println!("  foxflash.exe --list-known                  打印已记录的 EEPID 对照表");
    println!("  foxflash.exe --json <镜像.bin>             机器可读输出");
    println!("  foxflash.exe --help                        显示本帮助");
    println!();
    println!("输出字段:");
    println!("  MAC @0x00   NVM_COMPAT 高字节 @0x07   NVM 版本 @0x0A   Vendor @0x18");
    println!("  DeviceID @0x1A   Subsystem @0x1C   word 0x10 @0x20   EEPID/EtrackID @0x84");
    println!("  校验和字 @0x7E(word 0x3F)   AltMAC 指针 @0x6E(word 0x37)");
    println!("  换算：NVM word N <-> .bin byte 2N（igc_ethtool.c: first_word = offset >> 1）");
    println!();
    println!("0x84 的依据：用 Intel 官方驱动包 Release_31.2.2 标定，其镜像文件名自带 EEPID 后缀，");
    println!("  实测 u32@0x84 一比一命中，且与 nvmupdate.cfg 的 EEPID: 字段三方互证。");
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn print_json(rs: &[ImageInfo]) {
    println!("[");
    for (i, r) in rs.iter().enumerate() {
        println!("  {{");
        println!("    \"file\": \"{}\",", json_escape(&r.name));
        println!("    \"source\": \"{}\",", json_escape(&r.source));
        println!("    \"size\": {},", r.size);
        println!("    \"md5\": \"{}\",", r.md5);
        println!("    \"ok\": {},", r.ok);
        if r.ok {
            println!("    \"mac\": \"{}\",", r.mac);
            println!("    \"flash_idx\": \"0x{:02X}\",", r.flash_idx);
            println!("    \"capacity\": \"{}\",", r.flash_label);
            println!("    \"imgtype\": \"0x{:04X}\",", r.imgtype);
            println!("    \"nvmver\": \"0x{:04X}\",", r.nvmver);
            println!("    \"nvmver_label\": \"{}\",", r.nvmver_label);
            println!("    \"vendor\": \"0x{:04X}\",", r.vendor);
            println!("    \"devid\": \"0x{:04X}\",", r.devid);
            println!("    \"devid_label\": \"{}\",", r.devid_label);
            println!(
                "    \"subsystem\": \"{:04X}:{:04X}\",",
                r.subvendor, r.subdevice
            );
            println!("    \"eepid\": \"0x{:08X}\",", r.eepid);
            println!("    \"eepid_note\": \"{}\",", json_escape(&r.eepid_note));
        }
        let notes: Vec<String> = r.notes.iter().map(|n| format!("\"{}\"", json_escape(n))).collect();
        println!("    \"notes\": [{}]", notes.join(", "));
        println!("  }}{}", if i + 1 == rs.len() { "" } else { "," });
    }
    println!("]");
}

// ============================================================================
// 8. main
// ============================================================================

fn main() {
    set_console_utf8();
    let argv: Vec<String> = env::args().skip(1).collect();

    let mut recursive = false;
    let mut json = false;
    let mut targets: Vec<String> = Vec::new();

    for a in &argv {
        match a.as_str() {
            "-r" | "--recursive" | "-R" => recursive = true,
            "--json" => {
                json = true;
                JSON_MODE.store(true, Ordering::Relaxed);
            }
            "--list-known" => {
                show_known();
                return;
            }
            "-h" | "--help" | "/?" => {
                usage();
                return;
            }
            _ => targets.push(a.clone()),
        }
    }

    if targets.is_empty() {
        usage();
        println!();
        println!("提示：Windows 下可以直接把 .bin 文件拖到 foxflash.exe 图标上运行。");
        process::exit(1);
    }

    let entries = collect(&targets, recursive);
    if entries.is_empty() {
        process::exit(1);
    }

    let results: Vec<ImageInfo> = entries
        .into_iter()
        .map(|e| parse_image(&e.name, &e.source, e.data))
        .collect();

    if json {
        print_json(&results);
    } else {
        for r in &results {
            show_result(r);
        }
        show_compare(&results);
    }

    let bad = results.iter().filter(|r| !r.ok).count();
    if bad == results.len() {
        process::exit(1);
    }
}
