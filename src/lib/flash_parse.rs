// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// flash_parse.rs -- foxflash 的解析层：收集输入 + 解析 .bin 镜像头。
//
// 输入可以是：单个/多个 .bin、目录（只收 .bin）、.zip / .tar.gz / .tar 包。
// 输出 `ImageInfo`，交给 flash_report.rs 排版。

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use crate::archive::{is_image_name, read_tar, read_targz, read_zip, Entry};
use crate::md5::md5_hex;
use crate::nvm::{
    capacity_from_compat_hi, devid_label, imgtype_label, known_eepid, nvm_version_label,
    u16le_at, u32le_at, NVM_SUM,
};
use crate::term::note;

// ============================================================================
// 核心数据结构
// ============================================================================

pub struct ImageInfo {
    pub name: String,
    pub source: String,
    pub size: usize,
    pub md5: String,
    pub ok: bool,
    pub compat_hi: u8,
    pub capacity_label: String,
    pub imgtype: u16,
    pub imgtype_label: String,
    pub nvmver: u16,
    pub nvmver_label: String,
    pub mac: String,
    pub vendor: u16,
    pub devid: u16,
    pub devid_label: String,
    pub subvendor: u16,
    pub subdevice: u16,
    pub eepid: u32,
    pub eepid_note: String,
    /// word 0..0x3F 求和（含 word 0x3F 校验和字本身）
    pub checksum: u16,
    /// 是否等于内核定义的 NVM_SUM(0xBABA)
    pub checksum_ok: bool,
    /// word 0x37 = Alternate MAC 的字指针；0xFFFF 表示该区块已移除
    pub altmac_ptr: u16,
    pub altmac: String,
    pub notes: Vec<String>,
    pub data: Vec<u8>,
}

pub fn parse_image(name: &str, source: &str, data: Vec<u8>) -> ImageInfo {
    let mut r = ImageInfo {
        name: name.to_string(),
        source: source.to_string(),
        size: data.len(),
        md5: md5_hex(&data),
        ok: true,
        compat_hi: 0,
        capacity_label: String::new(),
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
    r.compat_hi = d[0x07];
    r.capacity_label = capacity_from_compat_hi(d[0x07]);
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
        r.checksum_ok = r.checksum == NVM_SUM;
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

    // ---- 自动体检 ----
    let d = &r.data;
    if d[0x07] != 0x0D && d[0x07] != 0x05 {
        r.notes.push(format!(
            "0x07=0x{:02X} 不在已知容量取值里（1MB=0x0D / 2MB=0x05），确认是不是 Foxville 镜像",
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
            (NVM_SUM as u32).wrapping_sub(r.checksum as u32) & 0xFFFF
        ));
    }

    if capacity_from_compat_hi(d[0x07]) != "未知"
        && imgtype_label(r.imgtype) != "未知"
        && capacity_from_compat_hi(d[0x07]) != imgtype_label(r.imgtype)
    {
        r.notes
            .push("0x07 与 0x20 指向的容量不一致，头部可能损坏".to_string());
    }
    if !matches!(r.vendor, 0x8086 | 0x17AA | 0x1028 | 0x8087) {
        r.notes.push(format!(
            "0x1C Vendor=0x{:04X} 非 0x8086，可能不是 Intel NVM 镜像",
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
// 输入收集
// ============================================================================

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
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
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

pub fn collect(targets: &[String], recursive: bool) -> Vec<Entry> {
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
            // 用闭包把「读文件失败」这件事一次说清，避免逐个分支重复
            match read_target(t) {
                Ok(entries) => out.extend(entries),
                Err(msg) => note(&msg),
            }
        } else {
            note(&format!("[!] 路径不存在：{}", t));
        }
    }
    out
}

/// 读一个显式给出的文件（非目录），按后缀决定是直读还是拆包。
fn read_target(t: &str) -> Result<Vec<Entry>, String> {
    let path = PathBuf::from(t);
    let raw = fs::read(&path).map_err(|_| format!("[!] 读取失败：{}", t))?;
    let s = t.to_ascii_lowercase();
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    if s.ends_with(".zip") {
        let got = read_zip(t, &raw);
        if got.is_empty() {
            return Err(format!("[!] zip 里没找到镜像文件：{}", t));
        }
        return Ok(got);
    }
    if s.ends_with(".tar.gz") || s.ends_with(".tgz") || s.ends_with(".gz") {
        return match read_targz(t, &raw) {
            Ok(got) => {
                if got.is_empty() {
                    Err(format!("[!] tar 里没找到镜像文件：{}", t))
                } else {
                    Ok(got)
                }
            }
            Err(e) => Err(format!("[!] gzip 解压失败 {} ({})", name, e)),
        };
    }
    if s.ends_with(".tar") {
        let got = read_tar(t, &raw);
        if got.is_empty() {
            return Err(format!("[!] tar 里没找到镜像文件：{}", t));
        }
        return Ok(got);
    }

    if !is_bin_name(&name) && !is_image_name(&name) {
        note(&format!(
            "[i] {} 不是 .bin，仍按镜像头解析，字段可能不准",
            name
        ));
    }
    Ok(vec![Entry {
        name,
        source: t.to_string(),
        data: raw,
    }])
}
