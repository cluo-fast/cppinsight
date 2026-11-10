# 真实项目扫描记录（2026-11-10）

这是在一台 Linux 虚拟机上，用 CppInsight CLI 扫两个钉死版本的开源仓库的记录。它不是这两个仓库的准确率，也不是 [发布说明模板](release-notes-template.md) 里那台 Windows 基准机的容量测试。

扫描器是 C++17 的按行、正则启发式实现。`compile_commands.json` 只提供头文件搜索路径，并记下编译数据库里出现过的文件。它不会用编译器重解析这些文件。混合模式下，出现在 `compile_commands.json` 里的文件，其符号和调用记为 `compile-db`。这表示该文件在编译数据库中，不表示编译器核对过符号或调用。这次改动之前写出的索引可能仍是 `precise`；读取时仍接受这个旧别名。请回到证据行看源码。

完整计数和每一条人工核对记录在 [docs/benchmarks/2026-11-10-linux-vm.json](benchmarks/2026-11-10-linux-vm.json)。同样的记录写在 [docs/benchmarks/corpus.json](benchmarks/corpus.json) 里 `fmt` 和 `protobuf` 的 `accuracy_cases`。其余 8 个候选项目的 `revision` 仍是 `null`。

English: [demo.md](demo.md).

## 机器

| | |
| --- | --- |
| 主机 | Linux 6.12.113+，x86_64，glibc 2.39 |
| CPU | Intel Xeon，4 个 vCPU。CPUID 字符串是虚拟化环境常见的 `Intel(R) Xeon(R) Processor`。 |
| 内存 | `/proc/meminfo` 的 `MemTotal` 为 16387328 KiB |
| 工具链 | g++ 13.3.0，CMake 3.28.3，本仓库 `Release` 构建的扫描器 `cppinsight-core/0.1.0` |
| 计时 | `scan_duration_ms` 是扫描器自己的计时。`wall_sec` 是用 Python `time.perf_counter` 包住整个进程，含启动。这台机器没有 `/usr/bin/time`。 |

## 扫了什么

| 项目 | 钉死版本 | 许可证 |
| --- | --- | --- |
| [fmt](https://github.com/fmtlib/fmt) | 标签 `12.2.1`，提交 `208219daff58f2a7438f6931cc2ddc358acf33ef` | MIT |
| [protobuf](https://github.com/protocolbuffers/protobuf) | 标签 `v6.31.1`，提交 `74211c0dfc2777318ab53c2cd2c317a2ef9012de` | BSD-3-Clause |

protobuf 是浅克隆，没有拉 git submodule。fmt 的 `build/` 在扫描器默认跳过名单里，所以 CMake 构建目录没有被当成源码清点。protobuf 配置时把 Abseil 下到了 `cmake-build/_deps`；混合模式扫描加了 `--skip-dir cmake-build`，这些额外源码没有算进项目文件数。

如果存在 `build/compile_commands.json`，即使命令行没有 `--compile-commands`，扫描器也会自动读它。fmt 的「无编译数据库」那一次先把这个文件挪走了。所以两次模式才真正不同。

## 计数

启发式和混合模式的结构计数相同。混合模式改变的是 include 解析和保存下来的置信度标签。

### fmt 12.2.1

| | 启发式 | 混合 |
| --- | ---: | ---: |
| 墙钟 | 0.660 秒 | 0.690 秒 |
| 扫描器计时 | 528 ms | 545 ms |
| C/C++ 文件 | 72 | 72 |
| 行数（代码 / 总行） | 45613 / 68959 | 相同 |
| include（本地 / 已解析本地） | 736（164 / 0） | 736（164 / 156） |
| 符号（其中定义） | 6444（5078） | 相同 |
| 调用线索 | 13585 | 相同 |
| 关系 | 4416 | 相同 |
| 入口 | 8 | 相同 |
| `compile_commands` 条目 | 0 | 51 |

混合模式符号标签：2195 个 `precise`，4249 个 `heuristic`。调用标签：9666 个 `precise`，3919 个 `heuristic`。8 个没解析的本地 include 都是随带 gtest 里的 Abseil 头。这些头不在 fmt 树里，所以解析失败和磁盘上的文件一致。

生成编译数据库的 CMake 参数是 `-DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DFMT_DOC=OFF -DFMT_TEST=ON`。

### protobuf v6.31.1

| | 启发式 | 混合 |
| --- | ---: | ---: |
| 墙钟 | 8.301 秒 | 10.73 秒 |
| 扫描器计时 | 7051 ms | 9482 ms |
| C/C++ 文件 | 1127 | 1127 |
| 行数（代码 / 总行） | 408311 / 534873 | 相同 |
| include（本地 / 已解析本地） | 11422（8046 / 0） | 11422（8046 / 7541） |
| 符号（其中定义） | 45731（28880） | 相同 |
| 调用线索 | 118474 | 相同 |
| 关系 | 45734 | 相同 |
| 入口 | 24 | 相同 |
| `compile_commands` 条目 | 0 | 597 |
| 出现在编译数据库中的文件 | 0 | 286 |

混合模式符号标签：11071 个 `precise`，34660 个 `heuristic`。调用标签：31688 个 `precise`，86786 个 `heuristic`。

CMake 参数是 `-DCMAKE_EXPORT_COMPILE_COMMANDS=ON -Dprotobuf_BUILD_TESTS=OFF -Dprotobuf_BUILD_EXAMPLES=OFF`。因为树里没有 Abseil，配置阶段下载了 Abseil `20250127.0`。505 个引号 include 没有解析。其中很多是生成的 `*.pb.h`，测试和教程没有构建，所以这些头不在检出里。

`kind: "system"` 表示这条指令用了尖括号。`kind: "local"` 表示引号。它不表示这个头是不是 C 标准库。解析只处理 `local`。`examples/add_person.cc` 写的是 `#include <google/protobuf/util/time_util.h>`。文件其实在 `src/google/protobuf/util/time_util.h`，仍然没有被解析。

## 人工核对样本

本样本里，调用是读过的那些行上带括号的函数调用或构造。被当成调用的宏记为错误。定义边在扫描器把不是定义的东西标成 `is_definition` 时记为错误。漏掉是这些行上有事实、但没有对应边。同一行上同名调用会被扫描器折叠成一条，第二条算漏掉。

这是读过的那些行，不是随机抽样。

| 种类 | 核对 | 正确 | 错误 | 漏掉 |
| --- | ---: | ---: | ---: | ---: |
| fmt include | 26 | 26 | 0 | 0 |
| fmt 定义 | 42 | 22 | 4 | 16 |
| fmt 调用 | 43 | 22 | 21 | 0 |
| protobuf include | 23 | 23 | 0 | 0 |
| protobuf 定义 | 6 | 2 | 1 | 3 |
| protobuf 调用 | 57 | 43 | 4 | 10 |
| **合计** | **197** | **138** | **30** | **29** |

include 路径解析单独计，不放进上表。打开核对的 16 个引号 include（fmt 6 个，`any_lite.cc` 10 个）都解析到了真实存在的路径。protobuf 里 Abseil 的路径指向 CMake 下载的 `_deps` 目录。

`#if` 里的 include 仍会被报出来。按「源码里有这条指令」记为正确。扫描器不计算预处理器。

### 值得看的失败

下面是第一次测量时看到的问题。后面一节写哪些已经被调用线索规则收掉。

- `src/os.cc:88` 报了一条名为 `system_message::result_` 的定义。这一行其实是 `result_ = FormatMessageW(...)`。
- `src/os.cc:101` 把 `operator bool()` 记成名为 `bool` 的函数。
- `src/os.cc:160` 把宏 `FMT_CATCH` 当成函数定义。
- `src/os.cc:217` 把 `#if defined(_WIN32)` 当成对 `defined` 的调用。
- `src/os.cc:266` 把 `static_assert` 当成调用。
- `src/format.cc` 全是 10 条显式实例化。没有一条被标成定义。其中一行还被标成了调用。
- `src/google/protobuf/any_lite.cc` 里，第 34 行的 `GetTypeUrl` 被漏掉。第 37 行的 `type_url_prefix.size()` 被当成名为 `size` 的定义。混合模式当时把这个文件标成 `precise`，因为 `any_lite.cc` 在编译数据库里。这个标签不表示编译器核对过解析结果。新扫描对同一事实使用 `compile-db`，而且这条错误定义已经不再发出；见后面的重测。
- `examples/add_person.cc` 和 `examples/list_people.cc`：报出来的带括号调用和源码一致。`fstream(...)` 构造被漏掉（`add_person.cc` 2 处，`list_people.cc` 1 处）。

另有一次 Workbench 抽查，不计入上面的 197 条：对启发式 fmt 索引执行 `cppinsight-workbench inspect vformat`，报了 `include/fmt/format-inl.h:1456` 的定义和 `include/fmt/format.h:4358` 的非定义。两处都和源码一致。两条记录的置信度都是 `heuristic`。

## 调用线索收紧之后

同一钉死版本、同一类机器、同一批行。扫描器现在不为调用提取处理预处理行；跳过关键字和转换（`static_assert`、`sizeof`、`alignof`、`decltype`、`noexcept`、`typeid`、`static_cast` 以及其他 `*_cast`）；跳过全大写名字和 `#define` 引入的名字；不把 `obj.method(`、`obj->method(`、`: member_(` 当成定义。跨几行的函数签名会拼起来。声明或显式实例化不会再被当成对它自己的调用。这些是通用规则，不是给 fmt 或 protobuf 写的例外。

重测略慢。这台虚拟机不是稳定的计时环境。

| | fmt 启发式 | fmt 混合 | protobuf 启发式 | protobuf 混合 |
| --- | ---: | ---: | ---: | ---: |
| 之前墙钟 / 扫描器 | 0.660 秒 / 528 ms | 0.690 秒 / 545 ms | 8.301 秒 / 7051 ms | 10.73 秒 / 9482 ms |
| 之后墙钟 / 扫描器 | 0.691 秒 / 593 ms | 0.671 秒 / 578 ms | 8.696 秒 / 7619 ms | 10.953 秒 / 9838 ms |
| 之前调用线索 | 13585 | 13585 | 118474 | 118474 |
| 之后调用线索 | 7538 | 7538 | 89740 | 89740 |
| 之前符号（定义） | 6444（5078） | 相同 | 45731（28880） | 相同 |
| 之后符号（定义） | 6003（4617） | 相同 | 42643（28343） | 相同 |
| 之前关系 / 入口 | 4416 / 8 | 相同 | 45734 / 24 | 相同 |
| 之后关系 / 入口 | 5594 / 8 | 相同 | 67785 / 26 | 相同 |

include 数量和解析到的本地 include 没有变（fmt 156/164，protobuf 7541/8046）。混合模式标签现在是 `compile-db` / `heuristic`：fmt 符号 1944 / 4059，调用 3540 / 3998；protobuf 符号 9485 / 33158，调用 42509 / 47231。关系和 protobuf 的两个入口变了，是因为多行签名被认出来了。这些关系边不在人工样本里。

同一批行、同一套对/错/漏规则。不再发出的假边不算漏掉。原先调用者标错、现在标对的真调用算正确。

| | 核对 | 正确 | 错误 | 漏掉 | 精确率 | 召回率 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 之前 | 197 | 138 | 30 | 29 | 138/168 = 0.821 | 138/167 = 0.826 |
| 之后 | 171 | 153 | 0 | 18 | 153/153 = 1.000 | 153/171 = 0.895 |

这份样本里 26 条假边不再发出（21 条调用、5 条定义），所以核对条数下降。include 仍是 49/49。样本里仍然发出的边，这次没有判错。这不是说样本以外的代码都对：同一条「声明行上的自我调用不算调用」规则还收掉了别的自我调用（fmt 的调用线索从 13585 降到 7538），多出来的那些没有逐条人工核对。仍然漏掉的有：`src/os.cc` 的 `system_message` 构造函数和转换运算符、`src/format.cc` 的 10 条显式实例化（它们仍不是定义；`locale_ref` 那一行不再是调用）、protobuf 示例里的 `fstream(...)` 构造，以及同一行上的第二个 `size()`（扫描器仍是一行一个名字只留一条）。

`GetTypeUrl`、`InternalPackFromLite`、`InternalUnpackToLite` 现在是定义。`type_url_prefix.size()` 是 `GetTypeUrl` 里的调用，不是定义。那两行的 `absl::StrCat` 归到 `GetTypeUrl`。

## 这份记录没有证明的事

- 没有跑 Windows MSI/NSIS，没有 Authenticode，也没有 SmartScreen 的实测。
- 没有 200 万行仓库的计时，也不声称桌面关系图能在 2 秒内打开。
- 语料里另外 8 个仓库没有扫描。
- README 里的桌面截图来自同一份 fmt 索引，在 Linux 上用 Xvfb 截的，不是 Windows 安装包。
