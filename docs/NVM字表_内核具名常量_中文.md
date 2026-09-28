# Intel Foxville (I225 / I226) NVM 字表 —— 内核具名常量中文对照

> 整理日期：2026-09-28
> 来源：Linux 内核源码（Intel 提交，GPL-2.0）
> - `drivers/net/ethernet/intel/igb/e1000_defines.h`
> - `drivers/net/ethernet/intel/e1000e/defines.h`
> - `drivers/net/ethernet/intel/igc/igc_defines.h`
>
> **Intel 没有公开发布过 I225/I226 的 NVM 镜像字节布局文档**（业内一般需 NDA）。
> 官方 NVM 更新包里那个 `Foxpond_Map_File_v01.txt` 只有 719 字节，内容仅是
> Alternate MAC 的覆盖脚本，不是布局说明。
> 所以这份内核字表是目前**公开可得、且带具名常量**的最强依据。

## 0. 换算规则（先记住这条）

```
NVM word N   <->   .bin 字节偏移 2N
```

每个 word 是 16 位**小端**存储：`word值 = byte[2n] | (byte[2n+1] << 8)`

三重独立证据：

1. `igc/igc_ethtool.c`：`igc_ethtool_get_eeprom_len()` 返回 `hw->nvm.word_size * 2`；
   `first_word = eeprom->offset >> 1`；注释原文
   *"Device's eeprom is always little-endian, word addressable"*。
   → 在线复核：`ethtool -e eth0 offset 0 length 128 raw on` 应与 .bin 前 128 字节一致。
2. `igc/igc_nvm.c` 的 `igc_validate_nvm_checksum()`：word 0..0x3F 求和须 = `NVM_SUM`。
   实测 35 个镜像 **31 个精确等于 0xBABA**（1/65536 的巧合不可能）。
3. `NVM_ALT_MAC_ADDR_PTR`(word 0x37) = 0x0119 → byte 0x232，本机四个备份在该处正是
   各自的真 MAC，公版镜像是 `FF:FF:FF:FF:FF:FF`。

---

## 1. NVM 字偏移表（按 word 升序）

| word | 字节偏移 | 内核具名常量 | 中文释义 | 出处 | 本机实测值 |
|---|---|---|---|---|---|
| 0x00 | 0x00–0x05 | `NVM_MAC_ADDR` | **MAC 地址**（占 3 个 word，共 6 字节） | igb | `60:BE:B4:02:68:XX` |
| 0x03 | 0x06–0x07 | `NVM_COMPAT` / `NVM_COMPATIBILITY_REG_3` | 兼容性字。bit `0x0800` = `NVM_COMPAT_LOM`（LAN On Motherboard） | e1000e / igb | 1MB=`0x0D20`，2MB=`0x0520` |
| 0x04 | 0x08–0x09 | `NVM_ID_LED_SETTINGS` | ID LED 设置 / SERDES 输出幅度 | e1000e / igb | `0xFFFF` |
| 0x05 | 0x0A–0x0B | `NVM_VERSION` | **NVM 版本字**（见 §3 解码） | igb | `0x1057` → 1.57 |
| 0x08 | 0x10–0x11 | `NVM_PBA_OFFSET_0` | PBA 板号指针**守卫字**，有效值 `NVM_PBA_PTR_GUARD = 0xFAFA` | e1000e / igb | `0xFAFA` |
| 0x09 | 0x12–0x13 | `NVM_PBA_OFFSET_1` | PBA 板号**字指针**（指向 PBA 字符串所在 word） | e1000e / igb | `0x0125` |
| 0x0B | 0x16–0x17 | `NVM_SUB_DEV_ID` | PCI **Subsystem Device ID** | igb | 公版 `0x0000`；OEM 版 `0x22D8` |
| 0x0C | 0x18–0x19 | `NVM_SUB_VEN_ID` | PCI **Subsystem Vendor ID** | igb | 公版 `0x8086`；OEM 版 `0x17AA` |
| 0x0D | 0x1A–0x1B | `NVM_DEV_ID` | PCI **Device ID**（15F3=I225-V 等） | igb | `0x15F3` |
| 0x0E | 0x1C–0x1D | `NVM_VEN_ID` | PCI **Vendor ID**（Intel = 0x8086） | igb | `0x8086` |
| 0x0F | 0x1E–0x1F | `NVM_INIT_CTRL_2` / `NVM_INIT_CONTROL2_REG` | 初始化控制字 2（含 Flow Control 位，见 §2） | igb / e1000e | `0x8200` |
| 0x12 | 0x24–0x25 | `NVM_CFG` | 配置字 | e1000e | — |
| 0x13 | 0x26–0x27 | `NVM_INIT_CTRL_4` | 初始化控制字 4 | igb | — |
| 0x14 | 0x28–0x29 | `NVM_INIT_CONTROL3_PORT_B` | 端口 B 初始化控制字 3 | e1000e / igb | — |
| 0x19 | 0x32–0x33 | `NVM_FUTURE_INIT_WORD1` | 预留未来初始化字 1；校验有效位 `0x0040` | e1000e | — |
| 0x1A | 0x34–0x35 | `NVM_INIT_3GIO_3` | 3GIO（PCIe）初始化字 3 | e1000e | — |
| 0x1C | 0x38–0x39 | `NVM_LED_1_CFG` | LED1 行为配置 | igb | — |
| 0x1F | 0x3E–0x3F | `NVM_LED_0_2_CFG` | LED0 / LED2 行为配置 | igb | — |
| 0x24 | 0x48–0x49 | `NVM_INIT_CONTROL3_PORT_A` | 端口 A 初始化控制字 3 | e1000e / igb | — |
| 0x37 | 0x6E–0x6F | `NVM_ALT_MAC_ADDR_PTR` | **Alternate MAC（备用 MAC）的字指针**。`0xFFFF` = 该区块已移除 | e1000e / igb | I225 `0x0119`；I226 `0xFFFF` |
| 0x3D | 0x7A–0x7B | `NVM_COMB_VER_PTR` | 组合版本（Combine Version）指针 | igb | — |
| 0x3E | 0x7C–0x7D | `NVM_ETS_CFG` | ETS（外部热传感器）配置 | igb | — |
| 0x3F | 0x7E–0x7F | `NVM_CHECKSUM_REG` | **校验和字**。word 0..0x3F 求和须 = `NVM_SUM(0xBABA)` | igb / e1000e / **igc** | — |
| 0x42 | 0x84–0x85 | `NVM_ETRACK_WORD` | **EtrackID（EEPID）低 16 位** | igb | `0x0182` |
| 0x43 | 0x86–0x87 | `NVM_ETRACK_HIWORD` | **EtrackID 高 16 位**，有效标志 `NVM_ETRACK_VALID = 0x8000` | igb | `0x8000` |
| 0x83 | 0x106–0x107 | `NVM_COMB_VER_OFF` | 组合版本数据偏移 | igb | — |

> 组合读法：`EtrackID = (word0x43 << 16) | word0x42`
> 例：`0x8000 << 16 | 0x0182` = `0x80000182`，也就是 nvmupdate.cfg 里写的 `EEPID: 80000182`。

---

## 2. 掩码与判定常量（不是偏移，是"怎么读 bit"）

| 常量 | 值 | 中文释义 |
|---|---|---|
| `NVM_SUM` | `0xBABA` | word 0..0x3F 求和的**目标值**；不符 = 镜像被改过 / 校验和过期 |
| `NVM_CHECKSUM_UNINITIALIZED` | `0xFFFF` | 校验和字尚未初始化 |
| `NVM_RESERVED_WORD` / `NVM_VER_INVALID` | `0xFFFF` | 保留字标记 / 版本号无效标记 |
| `NVM_PBA_PTR_GUARD` | `0xFAFA` | PBA 指针的守卫签名（word 0x08） |
| `NVM_ETRACK_VALID` | `0x8000` | EtrackID 高字有效标志 |
| `NVM_ETRACK_SHIFT` | `16` | 高字左移位数 |
| `NVM_COMPATIBILITY_BIT_MASK` | `0x8000` | 兼容性字的兼容位 |
| `NVM_COMPAT_LOM` | `0x0800` | **LAN On Motherboard** 位 ← 1MB / 2MB 镜像的差异位 |
| `NVM_COMPAT_VALID_CSUM` | `0x0001` | 兼容性字"校验和有效"位 |
| `NVM_MAJOR_MASK` / `NVM_MAJOR_SHIFT` | `0xF000` / `12` | 版本主号 |
| `NVM_MINOR_MASK` / `NVM_MINOR_SHIFT` | `0x0FF0` / `4` | 版本次号（**igb 布局**，Foxville 不适用，见 §3） |
| `NVM_IMAGE_ID_MASK` | `0x000F` | Image ID（igb 布局里占最低 4 位） |
| `NVM_NEW_DEC_MASK` | `0x0F00` | 新版十进制编码掩码 |
| `NVM_HEX_CONV` / `NVM_HEX_TENS` | `16` / `10` | 次版本号"十六进制→十进制"折算用 |
| `NVM_COMB_VER_MASK` / `NVM_COMB_VER_SHFT` | `0x00FF` / `8` | 组合版本 |
| `NVM_WORD0F_PAUSE_MASK` | `0x3000` | word 0x0F 的 Flow Control 掩码 |
| `NVM_WORD0F_PAUSE` | `0x1000` | 对称 PAUSE 使能 |
| `NVM_WORD0F_ASM_DIR` | `0x2000` | 非对称 PAUSE 方向 |
| `NVM_WORD1A_ASPM_MASK` | `0x000C` | word 0x1A 的 ASPM 位 |
| `NVM_WORD_SIZE_BASE_SHIFT` | `6` | NVM 容量左移基数（`word_size = BIT(size+6)`） |
| `NVM_MAX_RETRY_SPI` | `5000` | SPI 忙等待上限 |

### SPI 操作码（驱动与闪存通信用，不是镜像内容）

| 常量 | 值 | 中文释义 |
|---|---|---|
| `NVM_READ_OPCODE_SPI` | `0x03` | 读命令 |
| `NVM_WRITE_OPCODE_SPI` | `0x02` | 写命令 |
| `NVM_WREN_OPCODE_SPI` | `0x06` | 写使能锁存 |
| `NVM_RDSR_OPCODE_SPI` | `0x05` | 读状态寄存器 |
| `NVM_A8_OPCODE_SPI` | `0x08` | opcode bit3 = 地址 bit8 |
| `NVM_STATUS_RDY_SPI` | `0x01` | 状态寄存器"就绪"位 |

### igc 的 NVM 读写寄存器位（I225/I226 专用）

| 常量 | 值 | 中文释义 |
|---|---|---|
| `IGC_NVM_RW_REG_DATA` | `16` | EERD/EEWR 寄存器里数据位的偏移 |
| `IGC_NVM_RW_REG_DONE` | `2` | 读/写完成位 |
| `IGC_NVM_RW_REG_START` | `1` | 启动位 |
| `IGC_NVM_RW_ADDR_SHIFT` | `2` | 地址位左移位数 |
| `IGC_NVM_POLL_READ` | `0` | 轮询"读完成"模式标志 |
| `IGC_NVM_DEV_STARTER` | `5` | Dev_starter 版本 |
| `IGC_NVM_GRANT_ATTEMPTS` | `1000` | 获取 NVM 使用权的最大重试次数 |

---

## 3. 版本字怎么解（有个坑）

内核 `igb/e1000_82575.c` 的 `igb_get_fw_version()`：

```c
major = (w & NVM_MAJOR_MASK) >> NVM_MAJOR_SHIFT;      /* 0xF000, 12 */
minor = (w & NVM_MINOR_MASK) >> NVM_MINOR_SHIFT;      /* 0x0FF0, 4  */
minor = (minor / NVM_HEX_CONV) * NVM_HEX_TENS + (minor % NVM_HEX_CONV);
```

**但 Foxville 的打包方式和 igb 那一代不同，直接套内核掩码会解错：**

| 世代 | 布局 | 1.94 的 word 值 |
|---|---|---|
| igb / e1000e 老布局 | `(major << 12) \| (BCD(minor) << 4) \| image_id` | `0x1940` |
| **Foxville (I225/I226)** | `(major << 12) \| (0 << 8) \| BCD(minor)` | `0x1094` |

Foxville 的次版本号落在 **bit 0..7**，bit 8..11 恒为 0。
所以本项目用掩码 **`0x0FFF`** 而不是内核的 `0x0FF0` —— 差一个半字节，"1.94" 就变成 "1.09"。
实测 35 个镜像 bit8..11 全为 0，且按此解码与文件名版本 **24/24 完全吻合**（GH 仓库全量回归）。

---

## 4. 本项目自己归纳的字段（**内核里查不到名字，请勿当官方**）

| 字节 | 值 | 归纳结论 | 证据强度 |
|---|---|---|---|
| 0x07 | `0x0D` = 1MB，`0x05` = 2MB | 实为 `NVM_COMPAT` 高字节，差异位 = `NVM_COMPAT_LOM` | 35/35 相关，语义已由内核常量解释 |
| 0x20（word 0x10） | `0x8022` = 1MB，`0x80A2` = 2MB | 未知字段，**公开头文件里查不到名字** | 仅统计相关（35/35） |
| — | 闪存容量本身 | 驱动从硬件 EECD 寄存器读：`igc_base.c` → `size = FIELD_GET(IGC_EECD_SIZE_EX_MASK, eecd); nvm->word_size = BIT(size)`。**镜像里没有"容量"字段** | 内核确认 |

**结论：1MB / 2MB 本质上是硬件属性，镜像里那两个字节只是强相关的间接指标。**
选型请以 **EEPID(0x84) + 版本字(0x0A) + 0x07/0x20** 三者一起看为准。

---

## 5. 在线复核命令（ImmortalWrt / Linux）

```sh
# 永久 MAC，应等于 .bin 字节 0x00..0x05
ethtool -P eth0

# 在线读 NVM 前 128 字节，与 .bin 开头对比（word 编址，byte offset 直接对应）
ethtool -e eth0 offset 0 length 128 raw on | od -An -tx1
xxd -l 128 备份.bin

# PCI ID，应等于 word 0x0E / 0x0D
cat /sys/bus/pci/devices/0000:02:00.0/vendor     # 0x8086
cat /sys/bus/pci/devices/0000:02:00.0/device     # 0x15f3

# 驱动与固件字符串
ethtool -i eth0
```

---

## 6. 参考来源

1. **Linux 内核（Intel 提交，GPL-2.0）**
   https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/net/ethernet/intel/
   GitHub 镜像：https://github.com/torvalds/linux/tree/master/drivers/net/ethernet/intel/
2. Intel 官方 I225 NVM Update Package v1.00 内 `Foxpond_Map_File_v01.txt`（word 0x37 备用 MAC 指针）
3. Intel Ethernet Adapters and Devices User Guide
   https://edc.intel.com/content/www/us/en/design/products/ethernet/adapters-and-devices-user-guide/
4. 社区实测整理（word 0x00 MAC / word 0x3F 校验和）
   https://github.com/ahmadexp/i225-NVM-FLASH
