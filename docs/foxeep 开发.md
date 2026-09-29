# foxeep 开发说明

> 面向**改代码的人**。使用者请看 [`foxeep用法.md`](foxeep用法.md)；
> 偏移量依据请看 [`NVM原理与偏移依据.md`](NVM原理与偏移依据.md)。

**Intel 提交的 Linux 内核源码地址（本工具所有偏移量的一手出处）：**

```text
https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/net/ethernet/intel/
```

关键文件：`igb/e1000_defines.h`、`e1000e/defines.h`、`igc/igc_defines.h`，
以及 PBA 解码算法所在的 `igb/e1000_nvm.c`（`igb_read_pba_string_generic()`）。

---

## 1. 与 foxflash 的关系

| | `foxflash.exe` | `foxeep.exe` |
|---|---|---|
| 输入 | `.bin` 完整 flash 镜像（含 `.zip`/`.tar.gz` 直读） | `.eep` Shadow RAM 文本转储（也可吃 `.bin` 做比对） |
| 独有代码 | 手写 MD5 / DEFLATE / gzip / zip / tar | 无压缩代码，只解析文本 + PBA 解码 |
| 入口 | `src/foxflash.rs`（156 行） | `src/foxeep.rs`（166 行） |
| **共用模块** | `lib/term.rs`、`lib/repl.rs`、`lib/nvm.rs` | 同上，**指向同一批源文件** |

> ✅ **v1.1 已还清技术债。** v1.0 时代 `known_eepid()` / `devid_label()` /
> `nvm_version_label()` 在两个 `.rs` 里各存一份，改动要同步两边。
> v1.1 起这些全部挪进 `src/lib/nvm.rs`，两个入口用
> `#[path = "lib/nvm.rs"] mod nvm;` **指向同一个文件**，
> 于是「改一处、两边生效」——而且**仍然不需要 Cargo**，每个 exe 还是一条 `rustc` 命令。
> 顺带把 foxeep 的「已知 EEPID」提示从 12 条精简 `match` 升级成完整 25 条表。

## 2. 编译

```bat
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
cd 仓库根目录
rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o src/foxeep.exe src/foxeep.rs
```

Git Bash：

```sh
PATH="/c/Users/<用户名>/.cargo/bin:$PATH" rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o src/foxeep.exe src/foxeep.rs
```

编译会多出一个 `.pdb`，可删。正常应**零 warning**（共用模块里的死代码已用
`#![allow(dead_code)]` 压掉 —— 因为 `nvm.rs` 里有一部分函数只有 foxflash 用得到）。

## 3. 模块结构

| 文件 | 行数 | 职责 |
|---|---|---|
| `src/foxeep.rs` | 166 | **入口**：CLI 参数解析、双击/拖拽判定与分发、`run_once()` |
| `src/lib/eep_parse.rs` | 484 | `Kind`/`Src`/`Info`、`.eep` 文本与 `.bin` 的 word 还原、`analyze()` 全部体检项、输入收集、`--dump` 范围解析 |
| `src/lib/eep_report.rs` | 307 | `show_result` / `show_dump` / `show_compare` / `usage` / `print_json` |
| `src/lib/nvm.rs` | 207 | 字段常量 / 版本解码 / 型号表 / `KNOWN_EEPID` ← **与 foxflash 共用** |
| `src/lib/term.rs` | 202 | 终端基座 ← **共用** |
| `src/lib/repl.rs` | 113 | 交互模式 ← **共用** |

`src/lib/eep_parse.rs` 的文件头有一整块 **`.eep` 格式的实测说明**（含 word ↔ `.bin`
字节的对应关系），别删。

## 4. 解析要点

### 4.1 .eep 是文本，不是二进制

```text
每行 8 个 word，word 是 4 位十六进制、按数值书写
';' 开头为注释；官方包用 ";-------Range [0x00-0x3f]-----" 每 64 word 分一节
固定 0x800 = 2048 word（4 KB Shadow RAM）
```

**word i == .bin 的 u16 小端 @字节 2i。** 例如 `.bin` 前 6 字节 `60 BE B4 02 68 XX`
对应 `.eep` 前三 word `BE60 02B4 0E68` —— `0xBE60` 小端拆开正是 `60 BE`。
这与内核 `igc_ethtool.c` 的 *"Device's eeprom is always little-endian, word addressable"* 一致。

### 4.2 兼容性处理

- `parse_word()` 容忍 `0x` 前缀、1~4 位十六进制；解析不出来的计入 `bad_tokens` 并报 `[!]`。
- `raw.trim()` 同时吃掉 `\r`，所以 CRLF / LF 都行。
- `String::from_utf8_lossy` 兜底，非 UTF-8 字节不会 panic。
- `Range` 分节头会**核对**声明起点与实际累计 word 数是否一致（对不上说明文件被手工编辑过）。
- word 取值统一走 `word(&w, i)`：**越界返回 `0xFFFF`**，与 Shadow RAM 未编程区一致，
  省掉满地边界判断。

### 4.3 .bin 参与比对

`load_one()` 里：`.eep`/`.txt` 走文本解析，**其余一律当二进制**按 u16 小端转 word。
所以 `foxeep.exe a.eep b.bin` 能直接逐 word 比对。

## 5. 已踩过的坑

### 坑 1：PBA 字符串的字节序

内核 `igb_read_pba_string_generic()` 写的是 `part_num[2i] = w >> 8`（高字节优先），
但在 Foxville 上按该顺序解出的是**乱码**（`G23456-000`）；
按内存顺序（低字节优先）才是 `2G43650-XX`。

本工具用**内存顺序**。长度字的处理仍按内核（`ptr++`、`len--`、取 `len-1` 个 word），
这部分在本机得到验证：`w0x125 = 0x0006` → 5 word = 10 字节 = `2G43650-XX` 正好 10 字符。

**未在内核里找到 Foxville 专用的 PBA 分支**，所以「Foxville 的 PBA 区字节序与 igb 相反」
属实测归纳，不是文档结论。

### 坑 2：版本字解码不能用内核原掩码

内核 `NVM_MINOR_MASK 0x0FF0` 是 igb 老布局（次版本占 bit 4..11）：1.94 → `0x1940`。
Foxville 次版本占 **bit 0..7**：1.94 → `0x1094`。
解码函数在 `src/lib/nvm.rs`（与 foxflash 共用），掩码用 `0x0FFF`，
再套内核的 HEX→DEC 折算。用错会解成 "1.09"。

### 坑 3：提示必须走 `note()`

`--json` 的卖点是「stdout 是纯 JSON」。收集阶段任何 `[i]`/`[!]` 若直接 `println!`
就会插到 JSON 前面，严格解析器立刻报错。新增提示一律用 `term::note()`，
由它按 `JSON_MODE` 路由到 stdout / stderr。

### 坑 4：交互模式别误伤脚本

无参数启动时，只有**双击**（独占控制台）或 **stdin 是终端**才进交互模式；
被管道/重定向调用仍走老行为（打帮助、退出码 1）。判据在
`src/lib/term.rs::launched_by_double_click()`，详细理由见
[`foxflash 开发.md`](foxflash%20开发.md) §5 坑 5 —— 别把这里改成「无参数就交互」。

### 坑 5：`--json` 状态要每次任务重置

`run_once()` 一进来就 `term::set_json_mode(false)`，否则在交互模式里打过一次
`--json` 之后，后续每一轮的提示都会改道 stderr。新增进程级状态时照此办理。

## 6. 回归测试

```bat
:: A. 本机 .eep：MAC 60:BE:B4:02:68:XX，NVM 1.57，EEPID 0x80000182，校验和 OK，PBA 2G43650-XX
foxeep.exe "F:\倍控G31-1338\倍控G31-4LAN_immortalwrt-V25.12_系统备份\04-I225V_1MB固件备份\60BEB402680E.eep"

:: B. .eep vs 同名 .bin 应「完全相同」（本机这组零差异）
foxeep.exe "...\60BEB402680E.eep" "...\60BEB402680E.bin"

:: C. 官方 .eep vs 同名 .bin 应「10 个 word 不同」
foxeep.exe "F:\倍控G31-1338\intel_i225_i226_firmware\NVM\FXVL_15F3_V_1MB_1.89.eep" ^
           "F:\倍控G31-1338\intel_i225_i226_firmware\NVM\FXVL_15F3_V_1MB_1.89.bin"

:: D. 两个官方 .eep 应「1 个 word 不同」（w0x00D = 15F3 vs 15F2）
foxeep.exe "...\FXVL_15F3_V_1MB_1.89.eep" "...\FXVL_15F2_LM_1MB_1.89.eep"

:: E. 目录扫描：备份目录 4 个 .eep（跳过 4 个 .bin）
foxeep.exe "F:\倍控G31-1338\倍控G31-4LAN_immortalwrt-V25.12_系统备份\04-I225V_1MB固件备份"

:: F. --dump / --json
foxeep.exe "...\60BEB402680E.eep" --dump 0x3c-0x43
foxeep.exe "...\04-I225V_1MB固件备份" --json
```

交互模式（管道喂输入即可，不需要真的双击）：

```bat
:: G. 进交互、跑一次、退出
printf "exit\n" | foxeep.exe -i
printf "\"...\60BEB402680E.eep\" --dump 0x3c-0x43\nexit\n" | foxeep.exe -i

:: H. 先带参数跑一次再进交互（= 拖拽启动的代码路径）
printf "exit\n" | foxeep.exe -i "...\60BEB402680E.eep"
```

要检查的行为：

1. 管道模式下**不**进交互（`foxeep.exe x.eep < /dev/null` 应跑完即退）；
2. `exit` / `quit` / `q` 都能退出；EOF（不给 exit）也安静退出、退出码 0；
3. 输入不存在的路径后仍能继续下一轮；
4. `--json` 时 stdout 仍是纯 JSON（提示都在 stderr，含目录跳过的 `[i]`）。

## 7. 已知未解问题

1. **PBA 字节序**（见坑 1）：与内核 igb 实现相反，无公开依据。
2. **Intel 官方包里 `.eep` 与同名 `.bin` 内容不一致**（10 个 word 不同，且 `.bin`
   校验和无效而 `.eep` 有效）。成因未明 —— 可能是 `.eep` 从真机 dump、`.bin` 是发布版镜像。
3. **15F2 的 `.eep` EtrackID 是 `0x800002FC`（15F3 的值）**，与自身的 DeviceID 不自洽。
4. **`w0x10` 在公开头文件里无名**，`0x8022`/`0x80A2` 是统计归纳。官方 `.eep` 里出现过
   `0x809D`，工具会提示该值未确认。
5. `w0x040` 与 `w0x7F0` 在官方 `.eep` / `.bin` 之间**互换**（`0x8002` ↔ `0x807D`），
   这两个 word 的含义未知。
6. Shadow RAM 只覆盖 4 KB（`0x800` word），而 1MB flash 镜像有 524288 word ——
   比对只能覆盖前 4 KB，这是 `.eep` 格式本身的限制，不是工具缺陷。
7. 「双击检测」依赖 `GetConsoleProcessList`，只在 Windows 生效；非 Windows 编译时
   恒为 `false`（走命令行语义）。

## 8. 变更记录

| 版本 | 日期 | 内容 |
|---|---|---|
| 1.0 (rust) | 2026-09-28 | 首版：`.eep` 文本解析、字段解码、逐 word 比对、`--dump`、`--json` |
| **1.1** | **2026-09-29** | **双击启动的交互模式**（`exit` 退出、跑完回提示符可连续输入、拖拽不关窗）+ `-i` 强制交互；**源码拆成「入口 + 模块」**，`nvm`/`term`/`repl` 与 foxflash **共用同一份文件**，还清「两份 known_eepid / devid_label / nvm_version_label」技术债；EEPID 已知表由 12 条精简 `match` 换成共用的完整 25 条表；`--dump` 缺参数时明确报错返回 2 |

### v1.1 详细

**功能**

1. **双击即用**：无参数双击 `foxeep.exe` 直接进交互模式：

   ```text
   foxeep> 备份.eep 备份.bin
   ```

   一行可写多个文件/目录，支持引号包住带空格的路径；跑完回到提示符并提示
   「继续输入…或 exit 退出」；`exit` / `quit` / `q` 退出。
2. **拖拽启动不闪退**：把 `.eep` 拖到图标上，先跑完这一次，再转入交互模式。
3. **`-i` / `--interactive`** 强制进交互模式。
4. **命令行行为零变化**：`--dump` / `--json` / `-r` 语义与退出码保持；
   被脚本/管道调用时不会进交互、不会挂住。

**结构**

5. 与 foxflash 共用 `src/lib/nvm.rs`：`known_eepid()` / `devid_label()` /
   `nvm_version_label()` / `capacity_from_compat_hi()` 从此只有一处定义。
   `w0x03` 的容量标签也改走共用函数（以前是 `compat_label()` 自己一份 `match`）。
6. 共用 `src/lib/term.rs`（终端基座 + `note()`）与 `src/lib/repl.rs`（交互模式）。
7. 引入方式为 `#[path = "lib/xxx.rs"] mod xxx;` —— **不需要 Cargo**，
   编译命令仍然是原来那一条 `rustc`。

**顺手修的**

8. `--dump` 后面漏给范围参数时明确报错并返回 2（以前静默忽略该参数）。
9. 文档：本文件从「行号表」改为「模块 + 函数」表（行号会腐烂）。
