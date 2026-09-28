# NVM 原理与偏移依据

> 讲**为什么** —— 偏移量依据、容量判定、校验和、EEPID 语义、`.eep` 与 `.bin` 的关系。
> **本文档由 `foxflash` 与 `foxeep` 两个工具共用。**
>
> - `foxflash` 怎么用 → [`foxflash用法.md`](foxflash用法.md)
> - `foxeep` 怎么用 → [`foxeep用法.md`](foxeep用法.md)
>
> 文中出现**字节偏移**（`@0x84`）时是 `.bin` 视角，出现 **word 号**（`w0x42`）时是
> `.eep` 视角，两者用 `byte = word × 2` 换算。

---

## 1. 先说清楚：Intel 有没有官方文档

**没有。** Intel 没有公开发布过「I225 NVM 镜像字节布局」文档（业内一般要签 NDA 才有）。

Intel 官方包里那个 `Foxpond_Map_File_v01.txt`（`nvmupdate.cfg` 里 `EEPROM MAP:` 指向的
「官方映射文件」）只有 **719 字节**，内容只是一段 Alternate MAC 的覆盖脚本
（`BEGIN OVERWRITE / BEGIN POINTER / 0x37 0x00..0x0B`），**不是布局说明**。

但 Linux 内核里有 **Intel 自己提交的 NVM 字表**，是公开可得的最强依据（GPL）：

```text
https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/net/ethernet/intel/
```

- `igb/e1000_defines.h`
- `e1000e/defines.h`
- `igc/igc_defines.h`

## 2. 换算规则：word ↔ byte

```text
NVM word N   <->   .bin 字节偏移 2N        （每个 word 小端存储）
```

**三重独立证据：**

1. `igc/igc_ethtool.c`：`igc_ethtool_get_eeprom_len()` 返回 `hw->nvm.word_size * 2`，
   `first_word = eeprom->offset >> 1`，注释原文
   *"Device's eeprom is always little-endian, word addressable"*。
   → 在路由器上 `ethtool -e eth0 offset 0x0 length 0x90` 的输出，应当与 `.bin`
   的前 0x90 字节**逐字节一致**，可在线复核。
2. `igc/igc_nvm.c`：`igc_validate_nvm_checksum()` 对 word 0..0x3F 求和要求 = `NVM_SUM`。
   实测 **35 个镜像里 31 个精确等于 `0xBABA`**（1/65536 概率的巧合不可能）。
3. word `0x37` = `0x0119` → byte `0x232`，本机四个备份在那里放的正是各自的**真 MAC**；
   公版镜像放 `FF:FF:FF:FF:FF:FF`。官方 `Foxpond_Map_File_v01.txt` 里的 `0x37`
   与内核 `NVM_ALT_MAC_ADDR_PTR` 指向同一个东西。

## 3. 具名字表（摘核心字段）

完整版见 [`NVM字表_内核具名常量_中文.md`](NVM字表_内核具名常量_中文.md)，这里只列本工具用到的：

| 内核常量 | word | byte | 中文释义 |
|---|---|---|---|
| `NVM_MAC_ADDR` | 0x0000 | `0x00` | MAC 地址（占 3 word / 6 字节） |
| `NVM_COMPAT` | 0x0003 | `0x06` | 兼容性字；bit `0x0800` = `NVM_COMPAT_LOM`（主板集成） |
| `NVM_VERSION` | 0x0005 | `0x0A` | NVM 版本字 |
| `NVM_PBA_OFFSET_0` | 0x0008 | `0x10` | PBA 指针守卫字，值 = `NVM_PBA_PTR_GUARD 0xFAFA` |
| `NVM_PBA_OFFSET_1` | 0x0009 | `0x12` | PBA 板号**字指针** |
| `NVM_SUB_DEV_ID` | 0x000B | `0x16` | 子系统 Device ID |
| `NVM_SUB_VEN_ID` | 0x000C | `0x18` | 子系统 Vendor ID |
| `NVM_DEV_ID` | 0x000D | `0x1A` | PCI Device ID |
| `NVM_VEN_ID` | 0x000E | `0x1C` | PCI Vendor ID |
| `NVM_INIT_CTRL_2` | 0x000F | `0x1E` | 初始化控制字 2（含流控位） |
| `NVM_ALT_MAC_ADDR_PTR` | 0x0037 | `0x6E` | Alternate MAC 的**字指针**（`0xFFFF` = 区块已移除） |
| `NVM_CHECKSUM_REG` | 0x003F | `0x7E` | 校验和字 |
| `NVM_ETRACK_WORD` | 0x0042 | `0x84` | EtrackID 低 16 位 |
| `NVM_ETRACK_HIWORD` | 0x0043 | `0x86` | EtrackID 高 16 位（有效标志 `0x8000`） |

掩码 / 目标值：`NVM_SUM 0xBABA`、`NVM_PBA_PTR_GUARD 0xFAFA`、`NVM_ETRACK_VALID 0x8000`、
`NVM_COMPAT_LOM 0x0800`、`NVM_MAJOR_MASK 0xF000/SHIFT 12`、
`NVM_MINOR_MASK 0x0FF0/SHIFT 4`、`NVM_WORD_SIZE_BASE_SHIFT 6`。

## 4. `@0x84` 是 EtrackID —— 四个独立来源互证

不是猜的：

1. **Intel 官方驱动包** `Release_31.2.2\NVMUpdatePackage\I225\I225_NVMUpdatePackage_v1_00_Linux\Linux_x64\`
   两个镜像的**文件名自带 EEPID 后缀**，实测 u32@0x84 一比一命中：
   - `FoxPond1_I225_15F2_2MB_1p94_800003BB.bin` → `@0x84 = 0x800003BB`
   - `Foxpond1_I225_15F2_LM_1MB_1p94_800003BC.bin` → `@0x84 = 0x800003BC`
2. 同目录 **`nvmupdate.cfg`** 里写的 `EEPID: 800003BB / 800003BC` 是同一批值
   → 文件名、cfg、镜像头**三方互证**。
3. **第三方仓库** `cocolight/Intel-I226-V-NVM-Firmware`（已克隆到 `F:\倍控G31-1338\Intel-I226-V-NVM-Firmware`）
   的 README 里有一张 EtrackID 表。2026-09-28 用本工具实测该仓库全部 24 个镜像，
   README 写的值与实测 `@0x84` **逐条命中，无一例外**
   （I225-V 9 + I226-V 11 + 125B/125D 4）。
4. 第三方实际刷机实录里的 `nvmupdate.cfg`：`EEPID: 80000422` 配 `FXVL_125C_V_2MB_2.32.bin`，
   与本工具实测一致。

不解压就能自己验：

```bat
foxflash.exe Release_31.2.2\NVMUpdatePackage\I225\I225_NVMUpdatePackage_v1_00_EFI.zip
foxflash.exe F:\倍控G31-1338\Intel-I226-V-NVM-Firmware -r
```

## 5. 为什么 eeupdate 回答不了「1MB 还是 2MB」

`eeupdate64e.efi` 是**硬件工具**，官方 `eeupdate.txt` 全表 60+ 参数里：

- 与「文件」沾边的只有 `/DUMP`、`/FLASH_DUMP`（硬件 → 文件）；
- `/D`、`/CHECKIMAGE`、`/VERIFY`（文件 → 硬件），且**必须**挂 `/NIC=` 或 `/BUS=//DEV=`
  指定一块真卡，验证的是「这份镜像能不能刷到这块卡上」，不是「这份文件是什么」；
- **没有一个参数**能「指定一个 .bin，打印它的容量 / 版本 / EEPID」。

`/CHECKIMAGE <imagefile>` 官方原文：*"Checks if the currently running NVM has security
constraints applied. Verifies that the NVM in `<imagefile>` can be loaded over the
running NVM"* —— 它读的还是硬件。

更关键的是：**就算进 UEFI shell 重 dump 一次，文件大小也回答不了这个问题**。
本机 `60BEB40268XX.bin` 是 2,097,152 字节，看着像 2MB，但：

```text
size      2097152
前半 md5  35d995e0fb4f48a3d00b583ff43220d4
后半 md5  35d995e0fb4f48a3d00b583ff43220d4
完全相同  True
@0x07 = 0x0D   @0x20 = 0x8022   @0x0A = 0x1057 (NVM 1.57)
```

这是**地址回绕**：1MB 闪存被按 2MB 长度读，高位地址线回绕，同一份内容被读了两遍。
eeupdate 再 dump 一百次也还是这个结果 —— **它自己就是「2MB 假象」的来源**。

所以：判容量靠离线读镜像头，eeupdate 只用来确认硬件侧 EtrackID 对得上
（`/NIC=1 /ADAPTERINFO` 报的 EtrackID 应等于备份镜像 `@0x84` 的 `0x80000182`）。

## 6. NVM 校验和的原理

内核 `igc_validate_nvm_checksum()`（与 igb/e1000e 同构）：

```text
sum = Σ word[0x00..0x3F]   (16 位回绕加法)
有效  <=>  sum == NVM_SUM (0xBABA)
```

word `0x3F`（byte `0x7E`）是**补丁字**，出厂时被凑成让总和正好等于 `0xBABA`。
所以任何改动（改 MAC、改 Subsystem ID、改配置字）都必须重算这个字。

**实测：35 个镜像里 31 个精确等于 `0xBABA`。** 4 个不符的：

| 镜像 | 性质 |
|---|---|
| 2 个 OEM 改过的 1.89 | 已证实：改了 Subsystem ID（`0x17AA:0x22D8`，4 字节 @0x16–0x19）**没重算校验和** |
| `Foxpond1_I225_15F3_V_1MB_1p94.bin`（待刷） | 校验和 `0xBAED`，与 15F2 同版本镜像只差 word 0x0D(`+1`) 和 word 0x3F(`+50`)，两者不自洽。**推测** Intel 该批镜像刷写时由工具重算（驱动侧有 `igc_update_nvm_checksum`），**但无公开依据** |

这个体检项正是抓出「Vendor/SubVendor 位置标反」那个 bug 的关键 —— 没有它，
OEM 镜像的 4 字节差异根本不会被注意到。

## 7. 容量字段的语义（订正过）

- **byte `0x07` 不是「闪存容量索引」**，它是 `NVM_COMPAT` 字的**高字节**。
  1MB = `0x0D20`、2MB = `0x0520`，只差 **bit `0x0800`**，正是 e1000e 里定义的
  `NVM_COMPAT_LOM`（LAN On Motherboard）。跟容量仍是 35/35 完全相关，但语义是 LOM 位。
- **byte `0x232` 不是「另一口的 MAC」**，是 **Alternate MAC Address**。
- **闪存容量本身驱动根本不从镜像读**：`igc_base.c` 里
  `size = FIELD_GET(IGC_EECD_SIZE_EX_MASK, eecd); nvm->word_size = BIT(size)`，
  读的是**硬件 EECD 寄存器**。所以 1MB / 2MB 本质是**硬件属性**，
  镜像里那两个字节只是强相关的**间接指标**（35/35 相关，但语义不是容量）。
- byte `0x20`（word 0x10）在公开头文件里**查不到名字**，`0x8022` / `0x80A2` 仅是统计归纳。

## 8. 版本字怎么解

内核 `NVM_MAJOR_MASK 0xF000 / SHIFT 12`、`NVM_MINOR_MASK 0x0FF0 / SHIFT 4` 描述的是
**igb 老布局**：`(major<<12) | (BCD<<4) | image_id`，1.94 → `0x1940`。

**Foxville 不一样**：次版本占 **bit 0..7**，bit 8..11 恒为 0，1.94 → `0x1094`。
所以本工具掩码用 `0x0FFF` 而不是 `0x0FF0`，再套内核的 HEX→DEC 折算：

```text
minor = (minor_raw / 16) * 10 + (minor_raw % 16)      // 0x94 -> 94，不是 148
```

照抄内核原掩码会把 `0x1094` 解成 "1.09"。改完后 GitHub 仓库 24 个镜像回归 **24/24 全对**。

## 9. EEPID 不能单独当版本号用

```text
0x80000425 = I226-V 1MB 的 2.27 和 2.32
0x80000422 = I226-V 2MB 的 2.27 和 2.32
```

EEPID 标识的是**镜像内容族**，不是版本号。选型必须
**EEPID + `@0x0A` 版本字 + `@0x07`/`@0x20` 容量** 三者一起看。

## 10. 仍未有公开依据的部分

1. 待刷镜像 `0x800003FC` 校验和 `0xBAED ≠ 0xBABA` 的成因（见 §6）。
2. byte `0x20`（word 0x10）的官方字段名。
3. `0x06`、`0x08`–`0x09` 区间个别字节的含义，工具未解析，不予置评。
4. `0x1000` 起是 NVM 第二份副本（`0x1232 = 0x232 + 0x1000` 印证主备各一份）—— 属统计观察，
   未见官方说明。
5. PBA 字符串的字节序（见 §11.3）。
6. 官方包里 `.eep` 与同名 `.bin` 内容不一致的成因（见 §11.4）。

---

## 11. `.eep`（Shadow RAM 转储）与 `.bin` 的关系

> 这一节是 `foxeep` 的存在理由。实测日期 2026-09-28。

### 11.1 `.eep` 是什么

`eeupdate.txt` 对 `/DUMP` 的官方说明：

> *"Dumps EEPROM/Shadow RAM contents to a `*.eep` file. Dumps flash (if present) to a `*.bin` file"*

即 **`.eep` = Shadow RAM 转储，`.bin` = 完整 flash 转储**，两者不是同一个东西的两种表示。

`.eep` 是**纯文本**，不是二进制：

```text
本机 eeupdate /DUMP 产出（10496 字节 / 256 行，无注释）：
    BE60 02B4 0E68 0D20 FFFF 1057 FFFF FFFF
    FAFA 0125 602F 0000 8086 15F3 8086 8200

Intel 官方包里的（11583 字节 / 320 行，带分节注释）：
    ;
    ;-------Range [0x00-0x3f]--------------
    8C8C FBAA 7839 0D20 FFFF 1089 FFFF FFFF
    FAFA 0125 602F 22D8 17AA 15F3 8086 8200
```

规律：

- 每行 8 个 word，word 是 **4 位十六进制、按数值书写**；
- `;` 开头是注释，官方包每 64 word 插一个 `Range [0x??-0x??]` 分节头；
- 固定 **`0x800` = 2048 word**（4 KB Shadow RAM），8 word/行 → 256 数据行。

### 11.2 word ↔ byte 在这里同样成立

`.bin` 前 6 字节 `60 BE B4 02 68 XX` 对应 `.eep` 前三 word `BE60 02B4 0E68`：
`0xBE60` 小端拆开正是 `60 BE`。所以

```text
.eep word i   ==   .bin 的 u16 小端 @字节 2i
```

与 §2 的换算规则和内核 *"little-endian, word addressable"* 完全一致。
**实证**：本机 `60BEB40268XX.eep` 与同名 `.bin` 的 2048 个 word **零差异**，
两边校验和都是 `0xBABA`。

### 11.3 PBA 板号（有个字节序的坑）

算法取自内核 `igb_read_pba_string_generic()`（`igb/e1000_nvm.c`）：

```text
read(w0x08) 必须 == 0xFAFA            NVM_PBA_PTR_GUARD
ptr = read(w0x09)                    0xFFFF / 0x0000 -> 无效
len = read(ptr)                      0xFFFF / 0x0000 -> 未编程
ptr++, len--                         取 len-1 个 word
```

长度字的解释在本机得到验证：`w0x125 = 0x0006` → 取 5 word = 10 字节，
正好等于 `2G43650-XX` 的 10 个字符，后面紧跟 `0xFFFF`。

**但字节序反了**：内核写 `part_num[2i] = w >> 8`（高字节优先），
Foxville 上按该顺序解出的是乱码（`G23456-000`），按**内存顺序**（低字节优先）
才是 `2G43650-XX`。`foxeep` 用内存顺序。未在内核里找到 Foxville 专用分支，
故属实测归纳。

### 11.4 官方包里的 `.eep` 与同名 `.bin` 不是同一份内容

`FXVL_15F3_V_1MB_1.89.eep` vs 同名 `.bin`：**10 个 word 不同**

| word | .eep | .bin | 说明 |
|---|---|---|---|
| `0x000`–`0x002` | `8C8C FBAA 7839` | `A000 00C9 0000` | MAC：真机 MAC vs 官方占位 `00:A0:C9:00:00:00` |
| `0x009` | `0x0125` | `0x0119` | PBA 指针 |
| `0x010` | `0x809D` | `0x8022` | 镜像类型字 |
| `0x020` | `0x2004` | `0x300C` | 未知 |
| `0x03F` | `0x6ABC` | `0x74DD` | **校验和补丁字** |
| `0x040` / `0x7F0` | `0x8002` / `0x807D` | `0x807D` / `0x8002` | 这两个值**互换**了，含义未知 |
| `0x050` | `0x0001` | `0x0000` | 未知 |

而且两边校验和的正负相反：**`.eep` = `0xBABA` 有效，`.bin` = `0x74B6` 无效**。

→ **别假设 Intel 包里的 `.eep` 是同名 `.bin` 的等价表示。**

### 11.5 还有一个更奇怪的

```text
FXVL_15F3_V_1MB_1.89.eep  vs  FXVL_15F2_LM_1MB_1.89.eep
  结果：1 个 word 不同  ->  w0x00D   A=0x15F3   B=0x15F2
```

两个 `.eep` 只差 DeviceID 那一个 word，EtrackID 却**都是 `0x800002FC`**（15F3 的值），
而 15F2 的 `.bin` 才是 `0x800002FB`。

→ 那份 15F2 的 `.eep` 看起来是「从 15F3 的卡上 dump 出来、只改了 DeviceID」的产物，
**不能当作 15F2 真实固件的代表**。

### 11.6 覆盖范围的差异

`.eep` 只有 2048 word（4 KB），1MB flash 镜像有 524288 word。
所以 `.eep` ↔ `.bin` 比对只能覆盖**前 4 KB** —— 这是 `.eep` 格式本身的限制，
不是工具缺陷。前 4 KB 恰好包含了所有关键字段（MAC、ID、校验和、EtrackID），
所以够用于「备份是否一致」这类判断。
