# foxville-nvm-utils

**Offline, read-only inspection tools for Intel I225 / I226 (Foxville) NVM firmware images.**
Intel I225 / I226（代号 **Foxville**）网卡 NVM 固件的**离线只读**分析工具集。

> 只**读**文件，不碰你的网卡、不写任何东西。刷机与否由你决定。

[下载](#下载) · [快速上手](#快速上手) · [文档](#文档导航) · [编译](#编译) · [偏移量依据](#偏移量依据) · [License](#license)

---

## 为什么又造一个轮子

GitHub 上已有的两个同类项目都是**改 / 刷**向的：

| 项目 | 语言 | 干什么 |
|---|---|---|
| [opentimecard/foxville-nvm-tool](https://github.com/opentimecard/foxville-nvm-tool) | Go | 批量改 NVM 里的 MAC 地址 |
| [ahmadexp/i225-NVM-FLASH](https://github.com/ahmadexp/i225-NVM-FLASH) | C | 在树莓派上刷写 / 恢复 NVM、烧永久 MAC |

本项目的定位是它们前面的那一步：**先看清手上这份镜像到底是什么，再决定刷不刷**。
镜像不对就刷，是 I225 变砖最常见的原因。

## 能干什么

- **双击即用** —— 双击 exe 进交互模式，可连续看多份镜像，`exit` 退出；拖拽启动也不闪退
- **判 1MB / 2MB** —— 识破「1MB 闪存被按 2MB 长度读出」造成的 **2MB 假象**（前后半 MD5 相同 = 地址回绕）
- 解析完整字段：MAC / 版本 / 四组 PCI ID / PBA 板号 / Alternate MAC / EEPID(EtrackID)
- **NVM 校验和体检**（内核 `NVM_SUM 0xBABA`）—— 改过 Subsystem ID 却没重算校验和的 OEM 镜像会被直接抓出来
- 多镜像横向对照，一眼看出待刷镜像与当前固件的差异
- **跨格式比对**：`.eep`（Shadow RAM 文本转储）↔ `.bin`（完整 flash 镜像）逐 word 对比
- 直接读 `.zip` / `.tar.gz` 包内的镜像，不用先解压

## 两个工具

| 工具 | 处理对象 | 说明 |
|---|---|---|
| **`foxflash`** | `.bin` 完整 flash 镜像 | 解字段、体检、多镜像对照、**判 1MB / 2MB** |
| **`foxeep`** | `.eep` Shadow RAM 文本转储（也能吃 `.bin` 做比对） | 解字段、体检、**逐 word 比对** |

前缀 `fox` = Intel 内部代号 **Fox**ville。
两个 exe 都**可以双击启动**，进入可连续输入的小控制台（见下）。

## 快速上手

### 最省事：双击

**双击 `foxflash.exe` / `foxeep.exe` 就直接进交互模式**：

```text
========================================================================
  foxflash 2.2 (rust)   Intel I225/I226 (Foxville) NVM 镜像离线体检
========================================================================
把文件拖进这个窗口，或直接粘贴路径，回车即可；一行可以写多个，用空格分隔。
输入 help 查看完整用法，输入 exit 退出。

foxflash> _
```

- 把 `.bin` **拖进窗口中**（或复制路径粘贴）回车，当场出结果；
- 一行可以写多个文件或一个目录，写法和命令行完全一样
  （`Foxpond1_...bin 备份.bin`、`备份目录 -r`、`--list-known` 都行）；
- **跑完会回到提示符等你继续**，可以一口气看很多份，不用反复启停；
- 输入 `exit`（或 `quit` / `q`）退出，也可直接关窗口；
- 把文件**拖到 exe 图标上**启动时，跑完这一次同样**不会关窗**，会转入交互模式。

从 cmd / PowerShell 用命令行启动时行为不变：跑完就退，不干扰脚本与管道。
想在命令行里强制进交互模式，加 `-i`。

### 命令行

```bat
:: 看一份 flash 镜像
foxflash.exe 镜像.bin
foxflash.exe 目录 -r              :: 递归扫目录（只收 .bin）
foxflash.exe --list-known         :: 已知 EEPID 对照表

:: 看一份 .eep 转储
foxeep.exe 备份.eep
foxeep.exe 备份.eep 备份.bin      :: 两者逐 word 比对
foxeep.exe 备份.eep --dump 0x00-0x7f   :: 打印原始 word
```

两个工具都支持 `--json`（纯 JSON 走 stdout，提示信息走 stderr），方便批量校验。

## 文档导航

全部文档在 [`docs/`](docs)。

| 文档 | 面向 | 内容 |
|---|---|---|
| [`foxflash用法.md`](docs/foxflash用法.md) | 使用者 | 双击/交互模式、命令行用法、扫目录规则、输出字段、自动体检项、EEPID 对照 |
| [`foxeep用法.md`](docs/foxeep用法.md) | 使用者 | 双击/交互模式、`.eep` 解析、比对模式、PBA 解码、实测结论 |
| [`NVM原理与偏移依据.md`](docs/NVM原理与偏移依据.md) | 想复核结论的人 | **两工具共用**：偏移量依据与证据强度、word↔byte 换算三重证据、1MB/2MB 回绕、校验和原理、`.eep` 与 `.bin` 的关系、未解问题 |
| [`NVM字表_内核具名常量_中文.md`](docs/NVM字表_内核具名常量_中文.md) | 查字段 | 内核具名常量 → word → 字节偏移 的完整中文对照表 |
| [`foxflash 开发.md`](docs/foxflash%20开发.md) | 改代码的人 | 模块结构、编译、维护入口（只改一处）、已踩的坑、回归测试、变更记录 |
| [`foxeep 开发.md`](docs/foxeep%20开发.md) | 改代码的人 | 同上（foxeep 版） |

## 仓库结构

```
foxville-nvm-utils/
├── README.md            ← 本文件
├── LICENSE              GPL-3.0
├── src/
│   ├── foxflash.rs      入口：命令行解析 + 交互模式调度
│   ├── foxeep.rs        入口：命令行解析 + 交互模式调度
│   └── lib/             模块（两个入口各自按需引入）
│       ├── term.rs          终端基座：UTF-8 / 中文宽度 / 提示出口 / 双击检测   ← 共用
│       ├── repl.rs          双击启动的交互模式（REPL）                        ← 共用
│       ├── nvm.rs           字段定义、版本解码、EEPID 表（**唯一数据源**）     ← 共用
│       ├── md5.rs           手写 MD5
│       ├── deflate.rs       手写 DEFLATE / gzip
│       ├── archive.rs       zip / tar(.gz) 直读
│       ├── flash_parse.rs   foxflash：收集输入 + 解析 .bin
│       ├── flash_report.rs  foxflash：排版输出
│       ├── eep_parse.rs     foxeep：还原 word + 体检
│       └── eep_report.rs    foxeep：排版输出
└── docs/                全部文档（用法 / 原理 / 开发）
```

`src/lib/` 里的模块用 `#[path = "lib/xxx.rs"] mod xxx;` 引入，
`term` / `repl` / `nvm` 三个被两个工具**共用同一份源文件** ——
所以字段表和版本解码不会再出现「改一边忘一边」。仍然**不需要 Cargo**。

## 下载

**v2.2（推荐）** —— 双击交互模式 + 模块化重构（foxflash 2.2 / foxeep 1.1）：

**[foxville-nvm-utils-v2.2-win64.zip](https://github.com/cocolight/foxville-nvm-utils/releases/download/v2.2/foxville-nvm-utils-v2.2-win64.zip)**
（`foxflash.exe` + `foxeep.exe` + 两份用法文档 + LICENSE）

上一版 **v2.1**（单文件源码，**没有**交互模式）：
[foxville-nvm-utils-v2.1-win64.zip](https://github.com/cocolight/foxville-nvm-utils/releases/download/v2.1/foxville-nvm-utils-v2.1-win64.zip)（233 KB）

其他平台不用等打包，按下面「编译」一条 `rustc` 命令自己编即可 ——
不需要 Cargo、不需要联网。

## 编译

源码**零第三方依赖**（MD5 / DEFLATE / gzip / zip / tar 全部手写），
拆成「入口 + `lib/` 模块」只是为了好读，**不需要 Cargo** —— `rustc` 原生支持多文件模块，
每个 exe 仍然只出一条命令：

```bat
cd src
rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o foxflash.exe foxflash.rs
rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o foxeep.exe   foxeep.rs
```

也可以从仓库根目录编（`#[path = "lib/..."]` 是相对**入口文件所在目录**解析的，两种写法都行）：

```bat
rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o src/foxflash.exe src/foxflash.rs
```

| 源码 | 行数 |
|---|---|
| `src/foxflash.rs` | 156 |
| `src/foxeep.rs` | 166 |
| `src/lib/`（10 个模块） | ~2520 |

## 偏移量依据

**Intel 从未公开过 I225 / I226 NVM 镜像的字节布局文档**（业内一般要签 NDA）。
本工具的所有偏移量取自 **Intel 自己提交进 Linux 内核的驱动源码**：

```
https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/net/ethernet/intel/
```

具体是 `igb/e1000_defines.h`、`e1000e/defines.h`、`igc/igc_nvm.c`、`igb/e1000_nvm.c` 里的具名常量
（`NVM_MAC_ADDR`、`NVM_VERSION`、`NVM_DEV_ID`、`NVM_ALT_MAC_ADDR_PTR`、`NVM_CHECKSUM_REG`、
`NVM_ETRACK_WORD`……），换算关系是 **word N ↔ 字节 2N**。

这条换算有三重独立证据支撑（详见 [`NVM原理与偏移依据.md`](docs/NVM原理与偏移依据.md)）：

1. `igc_ethtool.c` 的 `first_word = eeprom->offset >> 1`，注释原文 "Device's eeprom is always little-endian, word addressable"
2. 内核校验和算法 `sum(word 0..0x3F) == 0xBABA`：实测 **35 个镜像 31 个精确命中**
3. `NVM_ALT_MAC_ADDR_PTR`(word 0x37) 指向处放的确实是 Alternate MAC，与 Intel 官方 `Foxpond_Map_File_v01.txt` 同源

**未解问题**（不藏着，都写在原理文档里）：待刷的 1MB/1.94 镜像校验和为 `0xBAED ≠ 0xBABA`，成因未定；
byte `0x20`（word 0x10）的官方字段名未知。

## 实测样本

工具在 **31 个**来自不同渠道（Intel 官方 NVM 升级包、第三方仓库、真机 dump）的镜像上做过回归，
容量 / 版本 / DeviceID / EEPID 四项全部自洽。

## License

**GNU General Public License v3.0 or later** —— SPDX: `GPL-3.0-or-later`（全文见 [`LICENSE`](LICENSE)）。

Copyright (C) 2026 cocolight

|  |  |
|---|---|
| ✅ | 商用、修改、再分发都允许 |
| ⚠️ | **必须署名** —— 保留版权声明与许可声明，并注明你改了哪里 |
| ⚠️ | **衍生作品必须同样以 GPL-3.0 开源** —— 不能拿去做闭源产品；分发二进制时要能提供对应源码 |
| ⚠️ | 不提供任何担保 |

一句话：**你可以拿它赚钱，但不能拿走它、也不能把它关起来。**

关于内核：偏移量依据取自 GPL-2.0 的 Linux 内核驱动源码，但本项目只引用了其中的**常量数值**
（事实性信息），未复制任何内核代码。内核源码出处已在上方显著标注。
