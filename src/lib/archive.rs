// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// archive.rs -- zip / tar(.gz) 容器读取：不做解压落盘，直接把包里的镜像字节捞出来。
//
// 场景：Intel 官方驱动包是 .zip *和* .tar.gz 双份发布的，镜像埋在好几层目录里。
// 用本模块就能 `foxflash.exe 官方包.zip` 一步到位，不用先手工解压再挑文件。
//
// 只实现到这个用途够用的程度：不支持 zip64、不支持加密、不支持多成员 gzip。

#![allow(dead_code)]

use crate::deflate::{gunzip, inflate};
use crate::term::note;

/// 从容器里捞出来的一个文件
pub struct Entry {
    pub name: String,
    /// 来源描述，形如 `包.zip::net/igc/xxx.bin`，用于在输出里标明出处
    pub source: String,
    pub data: Vec<u8>,
}

/// 包内/目录内**显式指定文件**时认这些后缀。
///
/// 注意跟 `flash_parse.rs` 里的 `is_bin_name` 区分：那个是**扫目录**用的，
/// 只认 `.bin`；这个是**包内/显式传参**用的，宽松一些。
pub fn is_image_name(n: &str) -> bool {
    let lower = n.to_ascii_lowercase();
    lower.ends_with(".bin")
        || lower.ends_with(".img")
        || lower.ends_with(".eep")
        || lower.ends_with(".dat")
}

// ============================================================================
// zip
// ============================================================================

pub fn read_zip(path: &str, data: &[u8]) -> Vec<Entry> {
    let mut out = Vec::new();
    // 找 End Of Central Directory（从尾部往前扫，最多扫 66KB 以容纳注释）
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
            // 这里在**收集阶段**，必须走 note()：--json 时改道 stderr，
            // 否则这行会插到 stdout 的 JSON 前面，严格解析器会报错。
            Err(e) => note(&format!("[!] 跳过 {} ({})", base, e)),
        }
    }
    out
}

fn u32le_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn u16le_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

// ============================================================================
// tar（未压缩）与 tar.gz（上面先 gunzip 再进这里）
// ============================================================================

pub fn read_tar(path: &str, data: &[u8]) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p + 512 <= data.len() {
        let name_bytes = &data[p..p + 100];
        // 连续两个全零块 = 归档结束（这里只判第一个字节够用）
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
        // 数据区按 512 字节对齐
        let blocks = size.div_ceil(512);
        p = dstart + blocks * 512;
    }
    out
}

/// tar.gz / tgz：gunzip 出 tar 再交给 read_tar
pub fn read_targz(path: &str, data: &[u8]) -> Result<Vec<Entry>, String> {
    let plain = gunzip(data)?;
    Ok(read_tar(path, &plain))
}
