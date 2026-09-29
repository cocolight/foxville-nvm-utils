// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// nvm.rs -- Foxville（Intel I225 / I226）NVM 字段定义与解码。
//
// **这是两个工具唯一的字段知识数据源**：foxflash（读 .bin）与 foxeep（读 .eep）
// 都从这里取偏移依据、型号表、容量标签、EEPID 对照表。
// 以前这些函数在两个 .rs 里各存了一份（改一处忘一处），拆分模块后合并掉了。
//
// 协议：GPL-3.0-or-later（见仓库根 LICENSE）
// 项目：倍控 G31-1338 四口机 I225-V 固件升级

#![allow(dead_code)] // 共用模块：两个入口各用一部分

// ============================================================================
// 0. 偏移量的一手依据（2026-09-28，来源：Intel 自己提交进 Linux 内核的驱动源码）
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

/// 内核 `NVM_SUM`：word 0..0x3F 求和的目标值
pub const NVM_SUM: u16 = 0xBABA;

/// 内核 `NVM_PBA_PTR_GUARD`：PBA 区有效守卫字
pub const NVM_PBA_PTR_GUARD: u16 = 0xFAFA;

/// 内核 `NVM_ETRACK_VALID`：EtrackID 高位有效标志
pub const NVM_ETRACK_VALID: u16 = 0x8000;

/// `eeupdate /DUMP` 产出的 Shadow RAM 固定长度（word）= 0x800 = 2048 word = 4KB
pub const SHADOW_RAM_WORDS: usize = 0x800;

// ============================================================================
// 1. 小端取值（两份源码里原本各写了一遍，现在只此一份）
// ============================================================================

pub fn u32le_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

pub fn u16le_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

// ============================================================================
// 2. 字段解码
// ============================================================================

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
pub fn nvm_version_label(w: u16) -> String {
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

/// PCI Device ID -> 型号名（15F3 / 15F2 / 15F8 / 125B / 125C / 125D）
pub fn devid_label(d: u16) -> String {
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

/// `NVM_COMPAT` 的**高字节** -> 容量结构标签。
///
/// 1MB 结构 0x0D、2MB 结构 0x05，两者只差 bit `0x0800`（`NVM_COMPAT_LOM`）。
/// `.bin` 视角这个值在 byte 0x07；`.eep` 视角它在 word 0x03 的高字节。
/// 两个工具走的是同一个函数，不会再出现「一边叫闪存索引、一边叫 COMPAT」的说法。
pub fn capacity_from_compat_hi(hi: u8) -> String {
    match hi {
        0x0D => "1MB".to_string(),
        0x05 => "2MB".to_string(),
        _ => "未知".to_string(),
    }
}

/// 观测到的镜像类型字（`.bin` @0x20 / `.eep` w0x10）—— 该字段在公开头文件里无名。
pub fn imgtype_label(t: u16) -> String {
    match t {
        0x8022 => "1MB".to_string(),
        0x80A2 => "2MB".to_string(),
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
/// 2MB 2.27 与 2.32 共用 0x80000422。要定版本，以 NVM 版本字为准
/// （.bin @0x0A / .eep w0x05）。
pub const KNOWN_EEPID: &[(u32, &str)] = &[
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
    (0x80000425, "FXVL_125C_V_1MB_2.27 或 2.32（I226-V / 1MB；两版本共用此 ID，看 NVM 版本字定版本）"),
    // ---- I226-V (125C) 2MB ----
    (0x8000028D, "FXVL_125C_V_2MB_2.14（I226-V / 2MB / NVM 2.14）"),
    (0x80000303, "FXVL_125C_V_2MB_2.17（I226-V / 2MB / NVM 2.17）"),
    (0x80000371, "FXVL_125C_V_2MB_2.22（I226-V / 2MB / NVM 2.22）"),
    (0x800003AD, "FXVL_125C_V_2MB_2.25（I226-V / 2MB / NVM 2.25）"),
    (0x80000422, "FXVL_125C_V_2MB_2.27 或 2.32（I226-V / 2MB；两版本共用此 ID，看 NVM 版本字定版本）"),
    // ---- I226-LM (125B) / I226-IT (125D) ----
    (0x80000424, "FXVL_125B_LM_1MB_2.32（I226-LM / 1MB / NVM 2.32）"),
    (0x80000421, "FXVL_125B_LM_2MB_2.32（I226-LM / 2MB / NVM 2.32）"),
    (0x80000433, "FXVL_125D_IT_1MB_2.32（I226-IT / 1MB / NVM 2.32）"),
    (0x80000431, "FXVL_125D_IT_2MB_2.32（I226-IT / 2MB / NVM 2.32）"),
];

/// 查表。这张表是唯一数据源：镜像解析时的「已知」提示与 `--list-known` 都走它，
/// 不会出现「改了表忘了改帮助」。
pub fn known_eepid(e: u32) -> String {
    KNOWN_EEPID
        .iter()
        .find(|(k, _)| *k == e)
        .map(|(_, v)| v.to_string())
        .unwrap_or_default()
}
