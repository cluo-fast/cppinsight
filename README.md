# CppInsight AI

CppInsight AI is a lightweight C/C++ project understanding assistant.

The current V1 foundation is a dependency-light command-line core that scans a C/C++ project and produces a local code inventory. The GUI, parser, database, and AI layers will build on top of this core.

## Current Features

- Scan a project directory recursively
- Detect common C/C++ source and header files
- Classify source, header, and inline files
- Count total, code, comment, and blank lines
- Summarize extensions and top-level layout
- Export a JSON inventory report

## Build

```powershell
cmake -S . -B build
cmake --build build --config Release
```

## Usage

```powershell
.\build\Release\cppinsight.exe scan D:\path\to\cpp-project
```

Optional JSON output:

```powershell
.\build\Release\cppinsight.exe scan D:\path\to\cpp-project --json report.json
```

The scan command prints a compact project inventory:

- source/header/inline file counts
- total/code/comment/blank line counts
- extension distribution
- top-level directory distribution
- first 20 indexed files

## Roadmap

- Symbol extraction for classes and functions
- SQLite project index
- AI code explanation and project Q&A
- Dear ImGui desktop viewer
