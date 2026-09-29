// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// flash_report.rs -- foxflash 的输出层：单镜像详情、多镜像对照、EEPID 表、帮助、JSON。
//
// 这里**只在非 --json 模式被调用**，所以可以直接 println!；
// 收集阶段的 [i]/[!] 提示走 term::note()，由它按 JSON_MODE 决定 stdout / stderr。

#![allow(dead_code)]

use crate::flash_parse::ImageInfo;
use crate::nvm::{u16le_at, u32le_at, KNOWN_EEPID};
use crate::term::{display_width, line, pad, thin_line, truncate};

fn human_size(n: usize) -> String {
    match n {
        1048576 => "1MB".to_string(),
        2097152 => "2MB".to_string(),
        _ => format!("{:.2} MB", n as f64 / 1048576.0),
    }
}

// ============================================================================
// 单镜像详情
// ============================================================================

pub fn show_result(r: &ImageInfo) {
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
        r.compat_hi, r.capacity_label
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

// ============================================================================
// 多镜像对照
// ============================================================================

pub fn show_compare(rs: &[ImageInfo]) {
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
        ("COMPAT 0x07", |r| {
            format!("0x{:02X} {}", r.compat_hi, r.capacity_label)
        }),
        ("类型 0x20", |r| {
            format!("0x{:04X} {}", r.imgtype, r.imgtype_label)
        }),
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
        // 偏移与标签按内核具名常量对齐（v2.2 订正：旧表把 0x18 标成 Vendor、
        // 0x1C/0x1E 标成 SubVendor/SubDevice，实际 0x16=SubDev 0x18=SubVen
        // 0x1A=DevID 0x1C=VenID 0x1E=InitCtrl2）。
        let fields: Vec<(&str, usize, usize)> = vec![
            ("0x07 COMPAT 高字节", 0x07, 1),
            ("0x0A NVM 版本", 0x0A, 2),
            ("0x16 SubDeviceID", 0x16, 2),
            ("0x18 SubVendorID", 0x18, 2),
            ("0x1A DeviceID", 0x1A, 2),
            ("0x1C VendorID", 0x1C, 2),
            ("0x1E NVM_INIT_CTRL_2", 0x1E, 2),
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
                println!("   {} {} -> {}", pad(label, 20), sa, sb);
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

// ============================================================================
// EEPID 表 / 帮助
// ============================================================================

pub fn show_known() {
    println!("已记录的 EEPID / EtrackID 对照表（与镜像解析共用源码里同一份 KNOWN_EEPID 表）：");
    println!("{}", thin_line());
    for &(e, note) in KNOWN_EEPID {
        println!("   0x{:08X}   {}", e, note);
    }
}

pub fn usage() {
    println!("{} {}", crate::APP, crate::VERSION);
    println!();
    println!("用法:");
    println!("  foxflash.exe <镜像.bin> [镜像2.bin ...]    查看单个或多个镜像");
    println!("  foxflash.exe <目录> [-r]                   扫描目录（加 -r 递归）");
    println!("  foxflash.exe <包.zip> / <包.tar.gz>        不解压，直接读包内镜像");
    println!("  foxflash.exe --list-known                  打印已记录的 EEPID 对照表");
    println!("  foxflash.exe --json <镜像.bin>             机器可读输出");
    println!("  foxflash.exe -i                            强制进入交互模式（等价于双击启动）");
    println!("  foxflash.exe --help                        显示本帮助");
    println!();
    println!("双击启动（无参数）:");
    println!("  双击 foxflash.exe 直接进入**交互模式**：把 .bin 拖进窗口或粘贴路径回车即可，");
    println!("  一行可以写多个（空格分隔），跑完会回到提示符等你继续，输入 exit 退出。");
    println!("  把 .bin 拖到 exe 图标上启动，跑完同样不会关窗，会转入交互模式。");
    println!();
    println!("输出字段:");
    println!("  MAC @0x00   NVM_COMPAT 高字节 @0x07   NVM 版本 @0x0A   Vendor @0x1C");
    println!("  DeviceID @0x1A   Subsystem @0x16/@0x18   word 0x10 @0x20   EEPID/EtrackID @0x84");
    println!("  校验和字 @0x7E(word 0x3F)   AltMAC 指针 @0x6E(word 0x37)");
    println!("  换算：NVM word N <-> .bin byte 2N（igc_ethtool.c: first_word = offset >> 1）");
    println!();
    println!("0x84 的依据：用 Intel 官方驱动包 Release_31.2.2 标定，其镜像文件名自带 EEPID 后缀，");
    println!("  实测 u32@0x84 一比一命中，且与 nvmupdate.cfg 的 EEPID: 字段三方互证。");
}

// ============================================================================
// JSON（stdout 纯 JSON）
// ============================================================================

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub fn print_json(rs: &[ImageInfo]) {
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
            println!("    \"flash_idx\": \"0x{:02X}\",", r.compat_hi);
            println!("    \"capacity\": \"{}\",", r.capacity_label);
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
        let notes: Vec<String> = r
            .notes
            .iter()
            .map(|n| format!("\"{}\"", json_escape(n)))
            .collect();
        println!("    \"notes\": [{}]", notes.join(", "));
        println!("  }}{}", if i + 1 == rs.len() { "" } else { "," });
    }
    println!("]");
}
