# foxflash 开发说明

> 面向**改代码的人**。
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

## 1. 源码构成（v2.2 起拆成模块）

v2.1 及以前是 `foxflash.rs` 单文件 1406 行。v2.2 起拆成「入口 + `lib/` 模块」，
但仍然**不需要 Cargo**（用 `#[path]` 引入，见下）。

| 文件 | 行数 | 职责 |
|---|---|---|
| `src/foxflash.rs` | 156 | **入口**：CLI 参数解析、双击/拖拽判定与分发、`run_once()` |
| `src/lib/term.rs` | 202 | 终端基座：控制台 UTF-8、中文宽度、`note()` 提示出口、双击检测、读行 ← **共用** |
| `src/lib/repl.rs` | 113 | 双击启动的交互模式（REPL）+ 命令行分词 ← **共用** |
| `src/lib/nvm.rs` | 207 | 字段常量 / 版本解码 / 型号表 / `KNOWN_EEPID` ← **共用，唯一数据源** |
| `src/lib/md5.rs` | 127 | 手写 MD5 |
| `src/lib/deflate.rs` | 293 | 手写 DEFLATE(inflate) 与 gzip |
| `src/lib/archive.rs` | 161 | zip / tar(.gz) 容器读取 |
| `src/lib/flash_parse.rs` | 336 | 输入收集 + `ImageInfo` + `parse_image()` |
| `src/lib/flash_report.rs` | 288 | `show_result` / `show_compare` / `show_known` / `usage` / `print_json` |
| `src/foxflash.exe` | — | 编译产物（x86_64 Windows，约 250 KB；**不入库**，走 GitHub Release） |

**模块怎么引入的**：

```rust
#[path = "lib/term.rs"]   mod term;
#[path = "lib/nvm.rs"]    mod nvm;
#[path = "lib/repl.rs"]   mod repl;
...
```

`#[path]` 是相对**入口文件所在目录**（`src/`）解析的，所以从仓库根目录或 `src/`
里编译都行。`term` / `repl` / `nvm` 三个文件被 `foxflash.rs` 和 `foxeep.rs`
**同时指向**，因此字段表和版本解码只有一处定义；每个 exe 编的是各带一份副本，
互不干扰（含 `term.rs` 里的 `JSON_MODE` 状态）。

因为是共用模块，每个入口只用得到其中一部分函数，剩下的在本 crate 里是死代码。
所以这几个共用模块开头都写了 `#![allow(dead_code)]`，别以为是代码有问题。

**零第三方依赖的含义**：MD5、DEFLATE/inflate、gzip、zip、tar **全部手写实现**，
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
正常应该**零 warning**（模块里的死代码已用 `allow(dead_code)` 压掉）；
如果刷出 warning，说明有真的没用到的 import，顺手清掉。

## 3. 谁该改哪里

| 想改的东西 | 改哪里 |
|---|---|
| 命令行参数、退出码、双击/拖拽的判定与分发 | `src/foxflash.rs` 的 `run_once()` / `main()` |
| 交互模式的提示语、横幅、退出命令 | `src/lib/repl.rs` |
| 终端宽度、UTF-8、`note()` 路由 | `src/lib/term.rs` |
| **任何字段偏移、型号名、版本解码、EEPID 表** | `src/lib/nvm.rs`（见 §4） |
| 单个镜像解析逻辑、体检项 | `src/lib/flash_parse.rs` 的 `parse_image()` |
| 输入收集（目录/zip/tar 怎么读） | `src/lib/flash_parse.rs` 的 `collect()` / `read_target()` |
| 输出排版 | `src/lib/flash_report.rs` |

## 4. 维护已知值 —— 只改一处

**不要用第二份列表。** v2.2 起这些值全部集中在 `src/lib/nvm.rs`，
`foxflash` 和 `foxeep` 共用同一份，不存在「改了 foxflash 忘了 foxeep」的问题了
（v2.1 及以前是两个 `.rs` 各存一份，是当时最大的技术债）。

| 位置（`src/lib/nvm.rs`） | 管什么 |
|---|---|
| `KNOWN_EEPID` | EEPID 对照表（25 条）。镜像解析时的「已知」提示、`--list-known`、`foxeep` 的「已知」提示，三处都读这一张 |
| `devid_label()` | DeviceID → 型号名（15F3 / 15F2 / 15F8 / 125B / 125C / 125D） |
| `capacity_from_compat_hi()` | `NVM_COMPAT` 高字节 → `1MB` / `2MB` 标签（`.bin` 看 byte 0x07，`.eep` 取 word 0x03 高字节，同一函数） |
| `imgtype_label()` | 镜像类型字 → 容量标签 |
| `nvm_version_label()` | 版本字解码 |
| `NVM_SUM` / `NVM_PBA_PTR_GUARD` / `NVM_ETRACK_VALID` / `SHADOW_RAM_WORDS` | 内核常量 |

另一个「唯一出口」是 `src/lib/term.rs` 的 `note()`：所有 `[i]`/`[!]` 提示都走它，
`--json` 时改走 stderr。

> 历史教训：早期 `--list-known` 另有一份硬编码 6 条列表，改了 `KNOWN_EEPID`
> 帮助不跟着变，两份数据会走偏。已合并为单一数据源；v2.2 又把两个工具的两份
> 也合并成了一份。

## 5. 已踩过的坑（改代码前必读）

### 坑 1：word ↔ byte 的换算

```text
NVM word N  <->  .bin 字节偏移 2N
```

写新字段时先确认是**字编号**还是**字节偏移**。内核常量（如 `NVM_ETRACK_WORD 0x0042`）
全是**字编号**，要 `× 2` 才是文件里的字节偏移（`0x84`）。这段说明写在
`src/lib/nvm.rs` 文件头的注释块里，别删。

### 坑 2：版本字解码不能用内核原掩码

内核 `NVM_MINOR_MASK 0x0FF0` 是 igb 老布局（次版本占 bit 4..11），
Foxville 的次版本占 **bit 0..7**，bit 8..11 恒为 0：

```text
igb 布局：1.94 -> 0x1940
Foxville：1.94 -> 0x1094
```

所以 `nvm_version_label()` 里掩码写 `0x0FFF` 而非 `0x0FF0`。用错会解成 "1.09"。
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

同一处错也留在「两文件差异摘要」的标签里（把 `0x18` 标成 Vendor、`0x1C/0x1E`
标成 SubVendor/SubDevice），**v2.2 已订正**为 `0x16 SubDeviceID / 0x18 SubVendorID /
0x1A DeviceID / 0x1C VendorID / 0x1E NVM_INIT_CTRL_2`。

### 坑 4：新增提示必须走 `note()`

`--json` 的卖点是「stdout 是纯 JSON，可以直接重定向给脚本」。任何在收集阶段
（`collect()` / `archive.rs` 的解包）新加的 `[i]` / `[!]` 提示如果直接 `println!`，
就会插到 JSON 前面，严格解析器立刻报错（实测 `json.loads` 直接抛 `Expecting value`）。

所以：**收集阶段的提示一律用 `term::note()`**，由它按 `JSON_MODE` 路由到 stdout / stderr。
`show_*()` / `print_json()` 里的 `println!` 不受影响（那些只在各自模式下跑）。
v2.2 顺手修掉了 `archive.rs` 里一处漏掉的 `println!`（zip 解压失败提示）。

### 坑 5：交互模式不能误伤脚本

交互模式（`repl.rs`）只在「人坐在终端前」时才进：

- 无参数 + **双击**（`term::launched_by_double_click()`）→ 进交互
- 无参数 + stdin 是终端 → 进交互
- 无参数 + stdin **不是终端**（管道/重定向）→ 打帮助 + 退出码 1（老行为，别改回去）

`launched_by_double_click()` 要求「独占控制台」**且**「stdin 是终端」两个条件同时成立。
加后一条是故意的：万一某环境误判成双击、而 stdin 其实是管道，进程会卡在那里等输入，
**调用它的脚本会直接挂死**。宁可漏判（双击后窗口照常关闭），也不能误判。
同理，`process::exit()` 只在命令行路径调用，交互模式的每一轮任务只返回退出码。

### 坑 6：`--json` 状态要每次任务重置

`JSON_MODE` 是进程级全局（`AtomicBool`）。`run_once()` 一进来就
`term::set_json_mode(false)`，否则在交互模式里打一次 `--json <file>`，
后面每一次输出都会把提示改道 stderr。新增全局状态时照此办理。

## 6. 回归测试

```bat
:: 三个样本目录，应分别解析出 4 / 24 / 4 个 .bin
foxflash.exe F:\倍控G31-1338\NVM升级包_I225V_UEFI
foxflash.exe F:\倍控G31-1338\Intel-I226-V-NVM-Firmware -r
foxflash.exe "F:\倍控G31-1338\倍控G31-4LAN_immortalwrt-V25.12_系统备份\04-I225V_1MB固件备份"

:: 版本解码回归：仓库 24 个镜像，解码结果应与文件名里的版本逐个对上（24/24）
foxflash.exe F:\倍控G31-1338\Intel-I226-V-NVM-Firmware -r

:: 其它
foxflash.exe --list-known          :: 打印完整 EEPID 对照表（25 条）
foxflash.exe 官方包.zip            :: zip 直读
foxflash.exe 官方包.tar.gz         :: tar.gz 直读
```

交互模式（管道喂输入即可自动跑完，不需要真的双击）：

```bat
:: 进交互、看一眼、退出
echo 60BEB402680E.bin | foxflash.exe -i
printf "exit\n" | foxflash.exe -i

:: 连续两次任务 + 带空格路径（引号）+ 帮助，最后退出
printf "\"F:\path with space\a.bin\"\nF:\x\b.bin\nhelp\nexit\n" | foxflash.exe -i

:: 先带参数跑一次再进交互（= 拖拽启动的代码路径）
printf "exit\n" | foxeep.exe -i 备份.eep
```

要检查的行为：

1. 管道模式下**不**进交互（`foxflash.exe x.bin < /dev/null` 应跑完即退）；
2. `exit` / `quit` / `q` 都能退出，EOF（不给 exit）也应安静退出、退出码 0；
3. 输入不存在的路径后仍能继续下一轮，不退出；
4. 退出码保持：`--help` = 0、`--list-known` = 0、`--dump` 参数错 = 2、
   路径全失败 = 1、无参数被脚本调用 = 1。

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
4. 「双击检测」依赖 `GetConsoleProcessList` 的行为，只在 Windows 上有效；
   非 Windows 编译时 `launched_by_double_click()` 恒为 `false`（走命令行语义）。

## 8. 变更记录

| 版本 | 日期 | 内容 |
|---|---|---|
| 1.0 (rust) | 2026-09-28 | 由 Python 版重写为 Rust，零依赖 |
| 2.0 | 2026-09-28 | 扫目录只收 `.bin`；修 I226 版本解码（支持 2.x）；补 125B/125D；EEPID 表改表驱动 |
| 2.1 | 2026-09-28 | **依据 Linux 内核源码订正**：新增 NVM 校验和体检、Alternate MAC；修 Vendor/Subsystem 位置反了的 bug；订正 `0x07` = `NVM_COMPAT` 高字节、`0x232` = Alternate MAC；版本解码改用内核折算公式（掩码改 `0x0FFF`） |
| 2.1 | 2026-09-28 | 由 `nvm_info` 更名为 **`foxflash`**（Foxville NVM），消除与 nvme / 通用 NVM 的歧义 |
| 2.1 | 2026-09-28 | 文档拆分：`foxflash用法.txt` → `foxflash用法.md`（用法与能力）+ 原理文档 + 本文件（开发）；`README.md` 改为目录导航 |
| 2.1 | 2026-09-28 | `--json` 模式下 `[i]`/`[!]` 提示改走 stderr（原本混在 stdout 的 JSON 前面，严格 JSON 解析器会直接报错） |
| **2.2** | **2026-09-29** | **双击启动的交互模式**：无参数双击进 REPL、`exit` 退出、跑完回到提示符可连续输入；拖拽启动跑完不关窗；新增 `-i` 强制交互。**源码拆成「入口 + 10 个 lib 模块」**，`term`/`repl`/`nvm` 与 foxeep 共用同一份文件，消灭「两份 known_eepid / devid_label / nvm_version_label」技术债；订正「差异摘要」表的字段标签；补掉 `archive.rs` 里一处漏走 `note()` 的 println |

### v2.2 详细

**功能**

1. **双击即用**：`main()` 用「本进程是否独占一个控制台 + stdin 是否终端」判定双击/拖拽
   （`term::launched_by_double_click()`），命中则进 `repl::run()`：
   - 无参数双击 → 直接进交互模式，`exit` / `quit` / `q` 退出；
   - 拖文件到图标 → 先照常跑完这一次，再转入交互模式，窗口不再一闪就关；
   - 交互模式里一行可以给多个文件/目录，支持引号包住带空格的路径；
   - 每跑完一次回到提示符并提示「继续输入…或 exit 退出」。
2. **`-i` / `--interactive`** 强制进交互模式（命令行用，也方便自动化测试）。
3. **命令行行为零变化**：`--help` / `--list-known` / `--json` / `-r` / `--dump`
   的语义与退出码全部保持；被脚本/管道调用时不会进交互、不会挂住。

**结构**

4. `foxflash.rs`（1406 行 → 156 行入口）+ 10 个 `lib/*.rs` 模块；用
   `#[path = "lib/xxx.rs"]` 引入，**仍然不需要 Cargo**，编译命令不变。
5. `nvm.rs` 合并了两个工具重复的 `KNOWN_EEPID` / `devid_label()` /
   `nvm_version_label()` / `u16le_at()` 等，成为唯一数据源。
   foxeep 的「已知 EEPID」提示顺带从 12 条精简 `match` 升级到完整 25 条表。
6. `capacity_from_compat_hi()` 统一了「`.bin` 看 byte 0x07」与「`.eep` 取 word 0x03 高字节」
   两种叫法。

**顺手修的**

7. 「差异摘要」表的字段标签订正（见 §5 坑 3 末）。
8. `archive.rs` 的 zip 解压失败提示改用 `note()`，`--json` 下不再污染 stdout（坑 4）。
9. `--dump` 后面漏给范围参数时明确报错并返回 2（以前是静默忽略）。
10. 文档：本文件重写为模块视角；行号表改为「模块 + 函数」表（行号会腐烂）。

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
4. EEPID 已知表改表驱动，且解析与 `--list-known` 共用同一份数据。
5. 单独指定非 `.bin` 文件时会提示「不是 .bin，字段可能不准」。
