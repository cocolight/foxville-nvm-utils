# foxflash 开发说明

> 面向**改代码的人**（含从原 `foxflash用法.txt` 迁移过来的「源码与重新编译」「同目录文件」
> 两节，以及变更记录）。
>
> - 用法 → [`foxflash用法.md`](foxflash用法.md)
> - 偏移量依据 → [`NVM原理与偏移依据.md`](NVM原理与偏移依据.md)
> - 完整内核具名字表 → [`NVM字表_内核具名常量_中文.md`](NVM字表_内核具名常量_中文.md)
> - 姊妹工具 `foxeep` 的开发说明 → [`foxeep 开发.md`](foxeep%20开发.md)

**Intel 提交的 Linux 内核源码地址（本工具所有偏移量的一手出处）：**

```text
https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/net/ethernet/intel/
```

GitHub 镜像：`https://github.com/torvalds/linux/tree/master/drivers/net/ethernet/intel/`
其中 `igb/e1000_defines.h`、`e1000e/defines.h`、`igc/igc_defines.h` 三个文件
是本工具字段定义的直接依据。

---

## 1. 源码构成

| 文件 | 说明 |
|---|---|
| `src/foxflash.rs` | 源码，单文件，1406 行，**零第三方依赖，只用 Rust 标准库** |
| `src/foxflash.exe` | 编译产物（x86_64 Windows，约 230 KB；**不入库**，走 GitHub Release） |
| `docs/foxflash用法.md` | 用户文档：命令行用法、输出字段、体检项 |
| `docs/NVM原理与偏移依据.md` | 偏移量依据、容量判定、校验和、EEPID 语义（**与 foxeep 共用**） |
| `docs/NVM字表_内核具名常量_中文.md` | 内核具名常量 → 字节偏移 的完整中文对照表 |
| `docs/foxflash 开发.md` | 本文件（开发者文档） |

**姊妹工具 `foxeep`**（`src/foxeep.rs` / `src/foxeep.exe` / `docs/foxeep用法.md` / `docs/foxeep 开发.md`）
读的是 `.eep`（Shadow RAM 文本转储）。两套代码里
`known_eepid()` / `devid_label()` / `nvm_version_label()` **各存了一份**，
改动要同步 —— 见 [`foxeep 开发.md`](foxeep%20开发.md) §1。

**零依赖的含义**：MD5、DEFLATE/inflate、gzip、zip、tar **全部手写实现**，
所以 `.exe` 拷到任何 Windows 机器上就能跑，不需要 Python / .NET / 运行库，也不需要联网。

> 早期还有 `foxflash.py` / `foxflash.bat` 作为交叉核对与拖拽启动器，现已不在。

## 2. 编译

Rust **没有卸载**，工具链在 `%USERPROFILE%\.cargo\bin`（rustup 装的 GNU 链）。
以下命令都在**仓库根目录**执行：

```bat
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o src/foxflash.exe src/foxflash.rs
```

Git Bash 里则写：

```sh
PATH="/c/Users/<用户名>/.cargo/bin:$PATH" rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o src/foxflash.exe src/foxflash.rs
```

编译完会多出一个 `.pdb`，可以删掉，不影响 exe。

## 3. 代码结构（按行号）

| 区块 | 行 | 内容 |
|---|---|---|
| 常量 / 提示输出口 | 20–34 | `APP`、`VERSION`、`JSON_MODE`（`AtomicBool`）、`note()` |
| 控制台 UTF-8 / 宽度计算 | 36–121 | `set_console_utf8`、`display_width`、`pad`、`truncate`（中文对齐用） |
| MD5 | 123–238 | `Md5`、`md5_hex`，手写实现 |
| DEFLATE / gzip | 240–512 | `BitReader`、`Huffman`、`inflate`、`gunzip`，手写实现 |
| 小工具 | 514–523 | `u32le_at` / `u16le_at`（小端取值） |
| 容器读取 | 525–631 | `Entry`、`read_zip`、`read_tar`（不解压直接读包内镜像） |
| 文件筛选 | 632–648 | `is_image_name`（显式传文件时用）、`is_bin_name`（扫目录时用，**只收 .bin**） |
| 核心数据结构 | 653–721 | `struct ImageInfo`，结构体上方有一整块**偏移量依据注释** |
| 字段标签 | 722–759 | `flash_idx_label`、`imgtype_label`、`devid_label` |
| EEPID 已知表 | 760–794 | `const KNOWN_EEPID`（**唯一数据源**）+ `known_eepid()` |
| 版本解码 | 829–840 | `nvm_version_label`，见 §5 坑 2 |
| 镜像解析 | 841–989 | `parse_image`，含 NVM 校验和、Alternate MAC、自动体检 |
| 输入收集 | 995–1091 | `collect_from_dir`、`collect`（提示统一走 `note()`） |
| 输出 | 1094–1344 | `show_result`、`show_compare`、`show_known`、`usage`、`print_json` |
| 入口 | 1349–1406 | `main` |

## 4. 维护已知值 —— 只改一处

**不要用第二份列表。** 以下每个入口都只有一个：

| 位置 | 管什么 |
|---|---|
| `KNOWN_EEPID`（760 行） | EEPID 对照表。镜像解析时的「已知」提示和 `--list-known` 都从这张表读 |
| `devid_label()`（738 行） | DeviceID → 型号名（15F3 / 15F2 / 125B / 125C / 125D） |
| `flash_idx_label()`（722 行） | byte `0x07` 的值 → 容量标签 |
| `nvm_version_label()`（829 行） | 版本字解码 |
| `note()`（28 行） | 所有 `[i]`/`[!]` 提示的唯一出口；`--json` 时改走 stderr |

> 历史教训：早期 `--list-known` 另有一份硬编码 6 条列表，改了 `KNOWN_EEPID` 帮助不跟着变，
> 两份数据会走偏。已合并为单一数据源。

## 5. 三个已踩过的坑（改代码前必读）

### 坑 1：word ↔ byte 的换算

```text
NVM word N  <->  .bin 字节偏移 2N
```

写新字段时先确认是**字编号**还是**字节偏移**。内核常量（如 `NVM_ETRACK_WORD 0x0042`）
全是**字编号**，要 `× 2` 才是文件里的字节偏移（`0x84`）。这段说明写在 `ImageInfo`
结构体上方的注释块里，别删。

### 坑 2：版本字解码不能用内核原掩码

内核 `NVM_MINOR_MASK 0x0FF0` 是 igb 老布局（次版本占 bit 4..11），
Foxville 的次版本占 **bit 0..7**，bit 8..11 恒为 0：

```text
igb 布局：1.94 -> 0x1940
Foxville：1.94 -> 0x1094
```

所以代码里掩码写 `0x0FFF` 而非 `0x0FF0`。用错会解成 "1.09"。
改这个函数后务必跑全量回归（见 §6）。

### 坑 3：Vendor / SubVendor 的位置

按内核常量，**不是**直觉上的顺序：

| 字节 | 常量 |
|---|---|
| `0x16` | `NVM_SUB_DEV_ID` |
| `0x18` | `NVM_SUB_VEN_ID` |
| `0x1A` | `NVM_DEV_ID` |
| `0x1C` | `NVM_VEN_ID` |
| `0x1E` | `NVM_INIT_CTRL_2`（**不是** SubDevice） |

历史 bug：早期把 `0x18` 当 VenID、`0x1C:0x1E` 当 Subsystem。Intel 公版镜像里
SubVen 和 Ven 都是 `0x8086`，所以一直没暴露；遇到 OEM 镜像（`0x17AA`）必读反。
是**校验和体检**把这个问题逼出来的（改过 Subsystem 就会 `≠ 0xBABA`）。

### 坑 4：新增提示必须走 `note()`，不要直接 `println!`

`--json` 的卖点是「stdout 是纯 JSON，可以直接重定向给脚本」。任何在 `collect()`
里新加的 `[i]` / `[!]` 提示如果直接 `println!`，就会插到 JSON 前面，
严格解析器立刻报错（实测 `json.loads` 直接抛 `Expecting value`）。

所以：**收集阶段的提示一律用 `note()`**，由它按 `JSON_MODE` 路由到 stdout / stderr。
`show_result()` / `show_compare()` 里的 `println!` 不受影响（那些只在非 json 模式跑）。

## 6. 回归测试

```bat
:: 三个样本目录，应分别解析出 7 / 24 / 4 个 .bin
foxflash.exe F:\倍控G31-1338\intel_i225_i226_firmware\NVM
foxflash.exe F:\倍控G31-1338\Intel-I226-V-NVM-Firmware -r
foxflash.exe "F:\倍控G31-1338\倍控G31-4LAN_immortalwrt-V25.12_系统备份\04-I225V_1MB固件备份"

:: 版本解码回归：仓库 24 个镜像，解码结果应与文件名里的版本逐个对上（24/24）
foxflash.exe F:\倍控G31-1338\Intel-I226-V-NVM-Firmware -r

:: 其它
foxflash.exe --list-known          :: 打印完整 EEPID 对照表
foxflash.exe 官方包.zip            :: zip 直读
foxflash.exe 官方包.tar.gz         :: tar.gz 直读
```

## 7. 已知未解问题

1. **待刷镜像校验和异常**：`Foxpond1_I225_15F3_V_1MB_1p94.bin`（EEPID `0x800003FC`）
   的 word 0..0x3F 求和 = `0xBAED` ≠ `NVM_SUM 0xBABA`。与 15F2 同版本镜像只差
   word 0x0D(`+1`) 和 word 0x3F(`+50`)，两者不自洽。推测 Intel 该批镜像在刷写时
   由工具重算校验和（驱动侧有 `igc_update_nvm_checksum`），**但无公开依据**。
   不影响 1MB 选型结论，留待确认。
   另有 2 个 OEM 1.89 镜像也是校验和不符 —— 那两个已证实是被改过 Subsystem ID
   （`0x17AA:0x22D8`）且没重算校验和，与上面这个性质不同。
2. **byte `0x20`（word 0x10）** 在公开头文件里查不到名字，`0x8022`/`0x80A2` 仅是统计归纳。
3. 闪存 1MB / 2MB 本质是硬件属性（驱动从 EECD 寄存器读），镜像里没有真正的容量字段。

## 8. 变更记录

| 版本 | 日期 | 内容 |
|---|---|---|
| 1.0 (rust) | 2026-09-28 | 由 Python 版重写为 Rust 单文件，零依赖 |
| 2.0 | 2026-09-28 | 扫目录只收 `.bin`；修 I226 版本解码（支持 2.x）；补 125B/125D；EEPID 表改表驱动（24 条） |
| 2.1 | 2026-09-28 | **依据 Linux 内核源码订正**：新增 NVM 校验和体检、Alternate MAC；修 Vendor/Subsystem 位置反了的 bug；订正 `0x07` = `NVM_COMPAT` 高字节、`0x232` = Alternate MAC；版本解码改用内核折算公式（掩码改 `0x0FFF`） |
| 2.1 | 2026-09-28 | 由 `nvm_info` 更名为 **`foxflash`**（Foxville NVM），消除与 nvme / 通用 NVM 的歧义 |
| 2.1 | 2026-09-28 | 文档拆分：`foxflash用法.txt` → `foxflash用法.md`（用法与能力）+ `foxflash原理.md`（依据）+ 本文件（开发）；`README.md` 改为目录导航 |
| 2.1 | 2026-09-28 | `--json` 模式下 `[i]`/`[!]` 提示改走 stderr（原本混在 stdout 的 JSON 前面，严格 JSON 解析器会直接报错） |

### v2.1 详细（原「依据 Linux 内核源码订正」）

1. 新增 NVM 校验和体检：word 0..0x3F 求和应 = `0xBABA`（内核 `NVM_SUM`）。
   不符说明镜像被改过或校验和过期 —— 实测抓出两个 OEM 改过的镜像。
2. 新增 Alternate MAC：经 word `0x37` 指针定位（官方 map 文件同源于此）。
3. 修正 Vendor / Subsystem 全部读反的 bug（见 §5 坑 3）。
4. 订正 `0x07` 的叫法：不是「闪存容量索引」，是 `NVM_COMPAT` 字的高字节，
   1MB(`0x0D20`) 与 2MB(`0x0520`) 只差 bit `0x0800` = `NVM_COMPAT_LOM`。
   跟容量仍是 35/35 完全相关，但语义改对了。
5. 订正 `0x232`：不是「另一口的 MAC」，是 Alternate MAC Address。

### v2.0 详细

1. 扫描目录时只认 `.bin` —— `.eep`/`.txt`/`.cfg`/`.md` 一律跳过
   （以前会混进来解析，还进汇总对照表，把真正该比对的 `.bin` 淹没掉）。
2. NVM 版本解码支持 2.x —— 修好了 I226 全系显示 `?` 的 bug
   （`0x2032` 现在正确显示 2.32，以前只认主版本 1）。
3. DeviceID 增加 `125B` = I226-LM、`125D` = I226-IT（以前显示「未知」）。
4. EEPID 已知表扩到 24 条，且解析与 `--list-known` 共用同一份数据。
5. 单独指定非 `.bin` 文件时会提示「不是 .bin，字段可能不准」。
