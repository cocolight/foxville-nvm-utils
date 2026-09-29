# foxeep.exe 用法

> Intel I225 / I226（Foxville）**Shadow RAM 转储（`.eep`）**解析与比对工具　|　版本 1.1（Rust）　|　2026-09-29
>
> **姊妹工具**：`foxflash.exe` 读完整 flash 镜像（`.bin` / `.zip` / `.tar.gz`），
> `foxeep.exe` 读 Shadow RAM 转储（`.eep`）。两者共用同一套内核偏移量定义
> （v1.1 起源码里也是**同一份文件** `src/lib/nvm.rs`，包括 EEPID 对照表）。
>
> - `.eep` 是什么、偏移量依据 → [`NVM原理与偏移依据.md`](NVM原理与偏移依据.md)
> - 完整内核具名字表 → [`NVM字表_内核具名常量_中文.md`](NVM字表_内核具名常量_中文.md)
> - 改代码 / 重新编译 → [`foxeep 开发.md`](foxeep%20开发.md)

---

## 1. 这是什么

`eeupdate /DUMP` 除了产出 `.bin`（完整 flash），还会产出 `.eep`（Shadow RAM 转储）。
`.eep` 是**纯文本文件**，把 NVM 的 word 数组按十六进制一行 8 个打印出来，
固定 `0x800 = 2048` word（4 KB）。

foxeep 做的事：把这份文本还原成 word 数组，按内核具名常量解出关键字段，
校验 NVM 校验和，并且**可以跟 `.bin` 逐 word 比对**。

- 单个 exe 约 210 KB，拷到任何 Windows 机器上就能跑；
- 不需要上机、不需要装 eeupdate。

## 2. 最省事的用法（v1.1 新增：双击即用）

**双击 `foxeep.exe`**，不用带任何参数，直接进交互模式：

```text
========================================================================
  foxeep 1.1 (rust)   Intel I225/I226 Shadow RAM 转储（.eep）解析与比对
========================================================================
把文件拖进这个窗口，或直接粘贴路径，回车即可；一行可以写多个，用空格分隔。
输入 help 查看完整用法，输入 exit 退出。

foxeep> 备份.eep
```

要点：

- 把 `.eep` **拖进这个窗口**（Windows 会自动把路径填到提示符后面），回车即出结果；
- 也可以复制路径粘贴，带空格的路径会自动带引号，工具能正确识别；
- **一行可以写多个文件或一个目录**，参数写法和命令行完全一致：

  ```text
  foxeep> 备份.eep 备份.bin
  foxeep> 备份.eep --dump 0x3c-0x43
  foxeep> "F:\倍控G31-1338\倍控G31-4LAN_immortalwrt-V25.12_系统备份\04-I225V_1MB固件备份"
  foxeep> help
  ```

- **跑完不会退出**，会回到 `foxeep>` 等你继续，可以连着比好几组文件；
- 输入 `exit`（或 `quit` / `q`）退出；直接关窗口也行；
- 把文件**拖到 exe 图标上**启动时，会先照常跑完这一次，然后同样**不关窗**、转入交互模式。

> 某次输入写错了（路径不存在之类）只会报一行 `[!]`，然后接着等下一次输入，不会退出。

从 cmd / PowerShell 用命令行启动时**行为完全不变**（跑完就退，不影响脚本与管道）。

## 3. 命令行用法

```bat
foxeep.exe 文件.eep                         :: 解析单个 .eep
foxeep.exe a.eep b.eep                      :: 两个 .eep 逐 word 比对
foxeep.exe a.eep b.bin                      :: .eep 与 flash 镜像比对（自动按 word 对齐）
foxeep.exe 目录 [-r]                        :: 扫描目录（只收 .eep），-r 递归
foxeep.exe 文件.eep --dump 0x00-0x7f        :: 打印原始 word
foxeep.exe --json 文件.eep                  :: 机器可读输出
foxeep.exe -i                               :: 强制进入交互模式（等价于双击启动）
foxeep.exe --help                           :: 帮助
```

> `-i` / `--interactive`：无条件进交互模式；若同时给了文件参数，会先跑完这些参数再进交互。
> 无参数启动时，**双击**、或 stdout 是终端，都会自动进交互模式；
> 被脚本/管道调用（stdin 不是终端）时保持老行为——打帮助并以退出码 1 结束，免得脚本挂住。
>
> `--json` 时 stdout **只有 JSON**，`[i]`/`[!]` 提示改走 stderr，可直接重定向给脚本解析。
>
> `--dump` 范围支持十六进制（`0x00-0x7f`）和十进制（`0-127`），也可以只给一个值（`0x40`）。

扫目录时**只收 `.eep`**，其它后缀会在开头提示「已跳过 N 个非 .eep 的文件」。

## 4. 输出哪些字段

| 输出项 | word | 说明 |
|---|---|---|
| MAC 地址 | `0x00` | 每 word 先低字节后高字节（内核 `igb_read_mac_addr()` 的顺序） |
| NVM_COMPAT | `0x03` | 高字节 `0x0D` = 1MB 结构，`0x05` = 2MB 结构（差 bit `0x0800` = `NVM_COMPAT_LOM`） |
| NVM 版本 | `0x05` | `0x1057` → 1.57；`0x1089` → 1.89；`0x2032` → 2.32 |
| 镜像类型 | `0x10` | 观测值 `0x8022` = 1MB / `0x80A2` = 2MB；**该字段在公开头文件里无名**，语义未确认 |
| PBA 板号 | 经 `0x08`/`0x09` | 见 §6 |
| SubDev / SubVen | `0x0B` / `0x0C` | OEM 镜像这里不是 `0x8086`（见过 `0x17AA:0x22D8`） |
| Device ID | `0x0D` | 15F3 = I225-V，15F2 = I225-LM，125B = I226-LM，125C = I226-V，125D = I226-IT |
| Vendor ID | `0x0E` | 一般 `0x8086` |
| InitCtrl2 | `0x0F` | 初始化控制字 2 |
| NVM 校验和 | `0..0x3F` 求和 | 应 = `0xBABA`（内核 `NVM_SUM`），补丁字在 `0x3F` |
| Alternate MAC | 经 `0x37` 指针 | `0xFFFF` = 该区块已移除 |
| **EEPID / Etrack** | `0x42` / `0x43` | **通常就是为了看这个** |
| 有效区间 | — | 最后一个非 `0xFFFF` 的 word，之后全是空白 |

## 5. 自动体检（末尾用 `[!]` 列出）

- word 数 ≠ 2048 → 文件可能被截断或不是 eeupdate 产出的；
- 有无法解析的 token（不是 1~4 位十六进制）；
- `Range [0x??-0x??]` 分节头声明的起点与实际累计 word 数对不上 → 文件可能被手工编辑过；
- NVM 校验和 ≠ `0xBABA`；
- VendorID ≠ `0x8086` → 可能不是 Intel NVM；
- DeviceID 不在 Foxville 已知表里；
- EtrackID 高位不是 `0x8000`（内核 `NVM_ETRACK_VALID`）→ 字段可能无效；
- `w0x10` 不是 `0x8022` / `0x80A2` → 提示容量以 `w0x03` 为准；
- AltMAC 指针越界。

## 6. PBA 板号怎么解（有个坑）

算法取自内核 `igb_read_pba_string_generic()`（`igb/e1000_nvm.c`）：

```text
read(0x08) 必须 == 0xFAFA        (NVM_PBA_PTR_GUARD)
ptr = read(0x09)                  (0xFFFF / 0x0000 -> 无效)
len = read(ptr)                   (0xFFFF / 0x0000 -> 未编程)
ptr++, len--
逐 word 取 len-1 个，拼成字符串
```

长度字的解释在本机得到验证：`w0x125 = 0x0006` → 取 5 个 word = 10 字节
= `2G43650-XX` 正好 10 个字符，后面紧跟 `0xFFFF`。

**字节序有坑**：内核写的是 `(w >> 8)` 优先，但 Foxville 上按该顺序解出的是乱码
（`G23456-000`），按内存顺序（低字节优先）才是 `2G43650-XX`。
本工具用**内存顺序**，并把这一点写进了源码注释。

## 7. 比对模式（最有价值的用法）

同时给两个文件时，逐 word 比对并列出所有不同的 word（最多列 24 条）。
`.bin` 会按「u16 小端」转成 word 数组，所以 `.eep` 和 `.bin` 可以直接互比。

长度不等时的提示分两种：

- 两边都在 Shadow RAM 量级（≤ 0x1000 word）却不等 → `[!]` 可能被截断；
- 一边是完整 flash 镜像 → `[i]` 属正常，只比对前 N 个 word。

## 8. 已实测到的三类结论

**A. 从真机 dump 的 `.eep` 与同名 `.bin` 完全一致。**
本机 `60BEB40268XX.eep` vs `60BEB40268XX.bin`：2048 个 word **零差异**，
两边校验和都是 `0xBABA`。→ 同源 dump 的两个文件可以互相印证。

**B. Intel 官方包里的 `.eep` 与同名 `.bin` 不是同一份内容。**
`FXVL_15F3_V_1MB_1.89.eep` vs 同名 `.bin`：**10 个 word 不同** ——
MAC（真机 MAC vs 官方占位 `00:A0:C9:00:00:00`）、PBA 指针（`0x0125` vs `0x0119`）、
`w0x010`、`w0x020`、`w0x03F`（校验和补丁字）、`w0x040`/`w0x7F0`（这两个值互换）、`w0x050`。
而且 `.eep` 校验和 `0xBABA` **有效**，同名 `.bin` 是 `0x74B6` **无效**。
→ **别假设 Intel 包里的 `.eep` 是同名 `.bin` 的等价表示。**

**C. 两个官方 `.eep` 只差 1 个 word，且 EtrackID 相同。**

```text
FXVL_15F3_V_1MB_1.89.eep  vs  FXVL_15F2_LM_1MB_1.89.eep
  结果：1 个 word 不同   ->   w0x00D   A=0x15F3   B=0x15F2
```

两者的 EtrackID 都是 `0x800002FC`（15F3 的值），而 15F2 的 `.bin` 才是 `0x800002FB`。
→ 那份 15F2 的 `.eep` 看起来是从 15F3 的卡上 dump 出来、只改了 DeviceID word 的产物，
**不能当作 15F2 真实固件的代表**。

## 9. 典型场景

```bat
:: 升级前留档：确认备份的 .eep 与 .bin 一致
foxeep.exe 60BEB40268XX.eep 60BEB40268XX.bin

:: 四个口一起看（MAC 应递增，EEPID 应相同）
foxeep.exe "…\04-I225V_1MB固件备份"

:: 看校验和与 EtrackID 那几个字
foxeep.exe 60BEB40268XX.eep --dump 0x3c-0x43
```
