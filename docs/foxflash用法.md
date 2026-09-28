# foxflash.exe 用法

> Intel I225 / I226（Foxville）**完整 flash 镜像（`.bin`）离线**查看工具　|　版本 2.1（Rust）　|　2026-09-28
>
> **姊妹工具**：`foxflash` 读完整 flash 镜像（`.bin`/`.zip`/`.tar.gz`），
> **`foxeep`** 读 Shadow RAM 转储（`.eep`）。看 `.eep` 请换 foxeep。
>
> - 偏移量的一手依据 → [`NVM原理与偏移依据.md`](NVM原理与偏移依据.md)
> - 完整内核具名字表 → [`NVM字表_内核具名常量_中文.md`](NVM字表_内核具名常量_中文.md)
> - 改代码 / 重新编译 → [`foxflash 开发.md`](foxflash%20开发.md)

---

## 1. 这是什么

一个**零依赖**的单文件命令行小工具。不用上机、不用装 eeupdate，直接在 PC 上读出任意
`.bin` 固件镜像的关键字段 —— 尤其是 **EtrackID（EEPID）**。

- 约 230 KB 单文件 exe，拷到任何 Windows 机器上就能跑；
- 不需要 Python、.NET 或任何运行库，不需要联网；
- 支持不解压直读 `.zip` / `.tar.gz` / `.tgz` 里的镜像。

## 2. 最省事的用法

把一个（或按住 Ctrl 选多个）`.bin` 文件**直接拖到 `foxflash.exe` 图标上松手**，
会弹出窗口显示结果，看完按任意键关闭。

## 3. 命令行用法

```bat
foxflash.exe 镜像.bin                      :: 查看单个镜像
foxflash.exe 备份.bin 升级镜像.bin          :: 多个镜像，自动出对照表
foxflash.exe 目录 [-r]                     :: 扫描目录，-r 为递归
foxflash.exe 官方包.zip                    :: 不解压，直接读 zip 里的镜像
foxflash.exe 官方包.tar.gz                 :: 不解压，直接读 tar.gz 里的镜像
foxflash.exe --list-known                  :: 打印已记录的 EEPID 对照表（24 条）
foxflash.exe --json 镜像.bin               :: 机器可读输出（JSON）
foxflash.exe --help                        :: 帮助
```

> `--json` 时 stdout **只有 JSON**，「已跳过 N 个非 .bin」这类 `[i]`/`[!]` 提示改走 stderr，
> 可以直接 `foxflash.exe ... --json > out.json` 交给脚本解析。

多个参数可以混着给：

```bat
foxflash.exe 备份目录 某个.zip 单独一个.bin
```

## 4. 扫目录时的文件筛选

- 扫目录**只收 `.bin`**；`.eep` / `.txt` / `.cfg` / `.md` 等一律跳过，
  开头会提示「已跳过 N 个非 .bin 的文件」，不参与解析、也不进汇总对照表。
- 想单独看某个 `.eep` 也可以：**把文件直接当参数给它**（不走目录过滤），
  工具会提示「不是 .bin，仍按镜像头解析，字段可能不准」。
  注意 `.eep` 是 shadow RAM dump（10496 字节），布局与完整 flash 镜像不同，字段仅供参考。

## 5. 输出哪些字段

基础信息：文件名 / 来源路径 / 大小 / MD5。

| 输出项 | 偏移 | 说明 |
|---|---|---|
| MAC 地址 | `@0x00` | 从真机 dump 出来的是真 MAC；公版镜像是占位 `00:A0:C9:00:00:00` |
| NVM_COMPAT 高字节 | `@0x07` | `0x0D` = 1MB 结构，`0x05` = 2MB 结构（两者只差 bit `0x0800`） |
| NVM 版本 | `@0x0A` | `0x1094` → 1.94；`0x1057` → 1.57；`0x2032` → 2.32 |
| SubDev / SubVen | `@0x16` / `@0x18` | OEM 镜像这里不是 `0x8086`（见过 `0x17AA:0x22D8`） |
| Device ID | `@0x1A` | 15F3 = I225-V，15F2 = I225-LM，125B = I226-LM，125C = I226-V，125D = I226-IT |
| Vendor ID | `@0x1C` | 一般 `0x8086` |
| 镜像类型 | `@0x20` | `0x8022` = 1MB，`0x80A2` = 2MB（与 `@0x07` 互证） |
| **EEPID / Etrack** | `@0x84` | **通常就是为了看这个** |
| NVM 校验和 | word 0..0x3F 求和 | 应 = `0xBABA` |
| Alternate MAC | 经 word 0x37 指针 | 公版镜像里是 `FF:FF:FF:FF:FF:FF` |

## 6. 自动体检（有问题在末尾用 `[!]` 列出）

- **NVM 校验和 ≠ `0xBABA`** → 镜像被改动过，或该批镜像刷写时才重算；
- **`@0x07` 与 `@0x20` 指向的容量不一致** → 头部可能损坏；
- **Vendor 不是 `0x8086`** → 可能不是 Intel NVM 镜像；
- **DeviceID 不在 Foxville 已知表里** → 型号对不上；
- **2MB 文件自动比对前后两半**：
  - 完全相同 → 同一份 1MB 镜像被 dump 了两遍，刷机要选 **1MB** 镜像；
  - 不同 → 确实是真实的 2MB 结构。

## 7. 多文件对照

给两个及以上文件时，末尾出一张「汇总对照」表；正好两个文件时，
额外给出**字段级差异摘要**（含整片不同字节数）。

## 8. 本机 1MB / 2MB 这个坑 —— 结论与操作

本机四个口共用一片闪存，`eeupdate /dump` 出来的备份是 2,097,152 字节（看着像 2MB），
但**前后两半逐字节完全相同**，本质是一份 1MB 结构的 NVM 被输出了两遍。

**所以本机必须用 1MB 镜像刷**（`Foxpond1_I225_15F3_V_1MB_1p94.bin` 是对的）。
工具会自动给出这个判断，不用自己数字节。

补充：eeupdate 自己回答不了这个问题 —— 它只认硬件，没有「读一个离线 .bin 再报容量」
的参数（官方 `eeupdate.txt` 全表 60+ 参数里没有），而且它 dump 出来的文件大小恰恰
就是这个 2MB 假象的来源。判容量只能靠离线读镜像头。

> 为什么会回绕、eeupdate 为什么读不了离线文件 → 见 [`foxflash原理.md`](foxflash原理.md)

## 9. EEPID 不能单独当版本号用

同一个 EEPID 会对应两个不同 NVM 版本：

```text
0x80000425 = I226-V 1MB 的 2.27 和 2.32
0x80000422 = I226-V 2MB 的 2.27 和 2.32
```

选型要 **EEPID + `@0x0A` 版本字 + `@0x07`/`@0x20` 容量** 三者一起看，
只报一个 EEPID 号不足以确定是哪一个版本。

## 10. 查已知 EEPID

```bat
foxflash.exe --list-known
```

打印完整 24 条对照表（表的唯一数据源就在源码 `KNOWN_EEPID` 里，比在任何文档里列都准）。

几条跟本机直接相关的：

| EEPID | 镜像 | 规格 |
|---|---|---|
| `0x800003FC` | `Foxpond1_I225_15F3_V_1MB_1p94` | 15F3 / 1MB / NVM 1.94 ← **待刷的升级镜像** |
| `0x80000182` | `60BEB40268XX.bin`（本机备份） | 15F3 / 1MB / NVM 1.57 ← 出厂老固件 |
| `0x800002FC` | `FXVL_15F3_V_1MB_1.89` | 15F3 / 1MB / NVM 1.89 |
| `0x800002F4` | `FXVL_15F3_V_2MB_1.89` | 15F3 / 2MB / NVM 1.89 |

同一个芯片型号，1MB 和 2MB 的 EEPID 是不一样的（1.89 那两个就是例子）。
