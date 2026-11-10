# Real-project scan notes (2026-11-10)

This is a record of two pinned open-source trees scanned with the CppInsight CLI on a Linux VM. It is not an accuracy rate for either repository, and it is not the Windows reference-machine benchmark in [the release-notes template](release-notes-template.md).

The scanner is a C++17 line and regular-expression heuristic. `compile_commands.json` supplies include search paths and marks files that appear in the compilation database. It does not re-parse those files with a compiler. In hybrid mode the stored label `precise` means compile-database-assisted. Confirm the evidence line in the source.

Full counts and every hand-checked row are in [docs/benchmarks/2026-11-10-linux-vm.json](benchmarks/2026-11-10-linux-vm.json). The same rows are the `accuracy_cases` for `fmt` and `protobuf` in [docs/benchmarks/corpus.json](benchmarks/corpus.json). The other eight corpus projects are still unpinned (`revision: null`).

中文: [demo.zh-CN.md](demo.zh-CN.md).

## Machine

| | |
| --- | --- |
| Host | Linux 6.12.113+, x86_64, glibc 2.39 |
| CPU | Intel Xeon, 4 vCPU. The CPUID string is the generic virtualized name `Intel(R) Xeon(R) Processor`. |
| Memory | 16387328 KiB reported by `MemTotal` |
| Toolchain | g++ 13.3.0, CMake 3.28.3, scanner `cppinsight-core/0.1.0` built `Release` from this tree |
| Clock | `scan_duration_ms` is the scanner's own timer. `wall_sec` is Python `time.perf_counter` around the process, including startup. `/usr/bin/time` is not installed here. |

## What was scanned

| Project | Pin | License |
| --- | --- | --- |
| [fmt](https://github.com/fmtlib/fmt) | tag `12.2.1`, commit `208219daff58f2a7438f6931cc2ddc358acf33ef` | MIT |
| [protobuf](https://github.com/protocolbuffers/protobuf) | tag `v6.31.1`, commit `74211c0dfc2777318ab53c2cd2c317a2ef9012de` | BSD-3-Clause |

Protobuf was a shallow clone with no git submodules. fmt's `build/` directory is on the scanner's default skip list, so the CMake build tree was not inventoried. Protobuf's configure wrote Abseil into `cmake-build/_deps`; that directory was passed as `--skip-dir cmake-build` on the hybrid run so those extra sources were not counted as project files.

If `build/compile_commands.json` exists, the scanner loads it even without `--compile-commands`. The fmt "without" run moved that file aside first. That is why the two fmt modes are actually different.

## Counts

Structural counts did not change between heuristic and hybrid. Hybrid changed include resolution and the stored confidence labels.

### fmt 12.2.1

| | Heuristic | Hybrid |
| --- | ---: | ---: |
| Wall | 0.660 s | 0.690 s |
| Scanner timer | 528 ms | 545 ms |
| C/C++ files | 72 | 72 |
| Lines (code / total) | 45613 / 68959 | same |
| Includes (local / resolved local) | 736 (164 / 0) | 736 (164 / 156) |
| Symbols (definitions) | 6444 (5078) | same |
| Call clues | 13585 | same |
| Relations | 4416 | same |
| Entry points | 8 | same |
| `compile_commands` entries | 0 | 51 |

Hybrid symbol labels: 2195 `precise`, 4249 `heuristic`. Call labels: 9666 `precise`, 3919 `heuristic`. The eight unresolved local includes are Abseil headers inside the bundled gtest copy. Those headers are not in the fmt tree, so leaving them unresolved matches the files on disk.

CMake for the database was `-DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DFMT_DOC=OFF -DFMT_TEST=ON`.

### protobuf v6.31.1

| | Heuristic | Hybrid |
| --- | ---: | ---: |
| Wall | 8.301 s | 10.73 s |
| Scanner timer | 7051 ms | 9482 ms |
| C/C++ files | 1127 | 1127 |
| Lines (code / total) | 408311 / 534873 | same |
| Includes (local / resolved local) | 11422 (8046 / 0) | 11422 (8046 / 7541) |
| Symbols (definitions) | 45731 (28880) | same |
| Call clues | 118474 | same |
| Relations | 45734 | same |
| Entry points | 24 | same |
| `compile_commands` entries | 0 | 597 |
| Files listed in the database | 0 | 286 |

Hybrid symbol labels: 11071 `precise`, 34660 `heuristic`. Call labels: 31688 `precise`, 86786 `heuristic`.

CMake was `-DCMAKE_EXPORT_COMPILE_COMMANDS=ON -Dprotobuf_BUILD_TESTS=OFF -Dprotobuf_BUILD_EXAMPLES=OFF`. Configure fetched Abseil `20250127.0` because Abseil was not already in the tree. 505 quoted includes stayed unresolved. Many of those are generated `*.pb.h` files that were not produced, because tests and the tutorial were not built.

`kind: "system"` means the directive used angle brackets. `kind: "local"` means quotes. It does not mean the header is or is not a C library header. Resolution only walks `local` includes. `examples/add_person.cc` includes `<google/protobuf/util/time_util.h>`. That file exists at `src/google/protobuf/util/time_util.h` and was still not resolved.

## Hand-checked sample

A call, for this sample, is a parenthesized call or constructor in the lines that were read. A macro invocation counted as a call is marked wrong. A definition edge is wrong when the scanner emitted `is_definition` for something that is not that definition. A miss is a fact in those lines with no matching edge. Same-name calls on one line are collapsed by the scanner; the second one is a miss.

This is the set of lines that were read, not a random draw.

| Kind | Checked | Correct | Wrong | Missed |
| --- | ---: | ---: | ---: | ---: |
| fmt includes | 26 | 26 | 0 | 0 |
| fmt definitions | 42 | 22 | 4 | 16 |
| fmt calls | 43 | 22 | 21 | 0 |
| protobuf includes | 23 | 23 | 0 | 0 |
| protobuf definitions | 6 | 2 | 1 | 3 |
| protobuf calls | 57 | 43 | 4 | 10 |
| **Together** | **197** | **138** | **30** | **29** |

Include resolution was scored separately, not inside that table. All 16 quoted includes that were opened (6 in fmt, 10 in `any_lite.cc`) resolved to a path that exists. For protobuf, Abseil paths point into the CMake-fetched `_deps` tree.

Includes inside `#if` were still emitted. They are scored correct as text. The scanner does not evaluate the preprocessor.

### Failures worth seeing

- `src/os.cc:88` emits a definition named `system_message::result_`. The line is `result_ = FormatMessageW(...)`.
- `src/os.cc:101` names `operator bool()` as a function `bool`.
- `src/os.cc:160` treats the `FMT_CATCH` macro as a function definition.
- `src/os.cc:217` treats `#if defined(_WIN32)` as a call to `defined`.
- `src/os.cc:266` treats `static_assert` as a call.
- `src/format.cc` is ten explicit instantiations. None were emitted as definitions. One line was also emitted as a call.
- In `src/google/protobuf/any_lite.cc`, `GetTypeUrl` (line 34) was missed. Line 37, `type_url_prefix.size()`, was emitted as a definition of `size`. Hybrid mode still labels that file `precise` because `any_lite.cc` is in the compilation database. The label does not mean the parse was checked by a compiler.
- `examples/add_person.cc` and `examples/list_people.cc`: the parenthesized calls that were emitted matched the source. The `fstream(...)` constructors were missed (2 in `add_person.cc`, 1 in `list_people.cc`).

A separate workbench check, not part of the 197: `cppinsight-workbench inspect vformat` on the heuristic fmt index reported a definition at `include/fmt/format-inl.h:1456` and a non-definition at `include/fmt/format.h:4358`. Both match those lines. Confidence on both records was `heuristic`.

## What this does not show

- No Windows MSI/NSIS run, no Authenticode, no SmartScreen result.
- No timing on a 2-million-line tree, and no claim that the desktop graph opens in 2 seconds.
- The other eight corpus repositories were not scanned.
- The desktop screenshots in the README are from this same fmt index on Linux, under Xvfb. They are not a Windows install.
