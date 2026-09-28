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
| 规模 | 1406 行，含手写 MD5 / DEFLATE / gzip / zip / tar | 917 行，**无压缩代码**，只解析文本 |
| 共有逻辑 | 版本解码、DeviceID 表、EEPID 表、内核偏移定义 | 同左（**两份代码里各有一份，改的时候要同步**） |

> ⚠️ `known_eepid()` / `devid_label()` / `nvm_version_label()` 在 `foxflash.rs` 和
> `foxeep.rs` 里**各存了一份**。foxflash 的 `KNOWN_EEPID` 是 24 条常量表，
> foxeep 的是精简版 `match`（12 条）。改动时两边都要改 —— 这是当前已知的技术债，
> 没有合并是因为两个 exe 都要保持「单文件零依赖」，拆公共库会引入多文件构建。

## 2. 编译

```bat
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o foxeep.exe foxeep.rs
```

Git Bash：

```sh
PATH="/c/Users/<用户名>/.cargo/bin:$PATH" rustc -O -C opt-level=s -C panic=abort -C strip=symbols -o foxeep.exe foxeep.rs
```

编译会多出一个 `.pdb`，可删。

## 3. 代码结构（按行号）

| 区块 | 行 | 内容 |
|---|---|---|
| 文件头注释 | 1–63 | **.eep 格式的实测说明**（含 word ↔ .bin 字节的对应关系），别删 |
| 常量 / 提示输出口 | 65–79 | `APP`、`VERSION`、`EXPECTED_WORDS`、`JSON_MODE`、`note()` |
| 控制台 UTF-8 / 分隔线 | 81–109 | `set_console_utf8`、`line`、`thin_line` |
| 数据结构 | 112–128 | `enum Kind`（Eep / Bin）、`struct Src` |
| 文本解析 | 130–214 | `parse_word`、`parse_range_header`、`parse_eep_text`、`parse_bin` |
| 字段解码 | 216–353 | `word`、`mac_from_words`、`nvm_version_label`、`devid_label`、`compat_label`、`imgtype_label`、`known_eepid`、`pba_string` |
| 分析 | 355–481 | `struct Info`、`analyze()`（含全部体检项） |
| 输出 | 483–745 | `show_result`、`show_dump`、`show_compare`、`print_json`、`usage` |
| 输入收集 | 747–846 | `is_eep_name`、`collect_from_dir`、`load_one`、`collect`、`parse_range`、`parse_num` |
| 入口 | 848–917 | `main` |

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

### 4.3 .bin 参与比对

`load_one()` 里：`.eep`/`.txt` 走文本解析，**其余一律当二进制**按 u16 小端转 word。
所以 `foxeep.exe a.eep b.bin` 能直接逐 word 比对。

## 5. 三个已踩过的坑

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
本工具掩码用 `0x0FFF`，再套内核的 HEX→DEC 折算。用错会解成 "1.09"。

### 坑 3：提示必须走 `note()`

`--json` 的卖点是「stdout 是纯 JSON」。收集阶段任何 `[i]`/`[!]` 若直接 `println!`
就会插到 JSON 前面，严格解析器立刻报错。新增提示一律用 `note()`，
由它按 `JSON_MODE` 路由到 stdout / stderr。

## 6. 回归测试

```bat
:: A. 本机 .eep：MAC 60:BE:B4:02:68:XX，NVM 1.57，EEPID 0x80000182，校验和 OK，PBA 2G43650-XX
foxeep.exe "F:\倍控G31-1338\倍控G31-4LAN_immortalwrt-V25.12_系统备份\04-I225V_1MB固件备份\60BEB40268XX.eep"

:: B. .eep vs 同名 .bin 应「完全相同」（本机这组零差异）
foxeep.exe "...\60BEB40268XX.eep" "...\60BEB40268XX.bin"

:: C. 官方 .eep vs 同名 .bin 应「10 个 word 不同」
foxeep.exe "F:\倍控G31-1338\intel_i225_i226_firmware\NVM\FXVL_15F3_V_1MB_1.89.eep" ^
           "F:\倍控G31-1338\intel_i225_i226_firmware\NVM\FXVL_15F3_V_1MB_1.89.bin"

:: D. 两个官方 .eep 应「1 个 word 不同」（w0x00D = 15F3 vs 15F2）
foxeep.exe "...\FXVL_15F3_V_1MB_1.89.eep" "...\FXVL_15F2_LM_1MB_1.89.eep"

:: E. 目录扫描：备份目录 4 个 .eep（跳过 4 个 .bin）
foxeep.exe "F:\倍控G31-1338\倍控G31-4LAN_immortalwrt-V25.12_系统备份\04-I225V_1MB固件备份"

:: F. --dump / --json
foxeep.exe "...\60BEB40268XX.eep" --dump 0x3c-0x43
foxeep.exe "...\04-I225V_1MB固件备份" --json
```

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
