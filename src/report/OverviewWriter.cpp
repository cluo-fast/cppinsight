#include "report/OverviewWriter.h"

#include <algorithm>
#include <fstream>
#include <sstream>
#include <stdexcept>
#include <unordered_map>
#include <unordered_set>

namespace cppinsight {
namespace {

std::string basenameTarget(const std::string& target) {
    const std::size_t slash = target.find_last_of("/\\");
    if (slash == std::string::npos) {
        return target;
    }
    return target.substr(slash + 1);
}

} // namespace

bool OverviewWriter::isEntryPointName(const std::string& name) {
    static const std::unordered_set<std::string> names = {
        "main", "wmain", "WinMain", "wWinMain", "DllMain", "_tmain", "_tWinMain", "mainCRTStartup"
    };
    return names.find(name) != names.end();
}

OverviewData OverviewWriter::build(const ScanResult& result) const {
    OverviewData overview;

    for (const SymbolInfo& symbol : result.symbols) {
        if (symbol.kind != "function" || !isEntryPointName(symbol.name)) {
            continue;
        }

        EntryPointCandidate candidate;
        candidate.name = symbol.name;
        candidate.file = symbol.filePath.generic_string();
        candidate.line = symbol.line;
        candidate.confidence = symbol.confidence.empty() ? result.confidence : symbol.confidence;
        candidate.reason = symbol.isDefinition
            ? "function definition matched common process entry name"
            : "function declaration matched common process entry name";
        overview.entryPoints.push_back(std::move(candidate));
    }

    std::sort(
        overview.entryPoints.begin(),
        overview.entryPoints.end(),
        [](const EntryPointCandidate& left, const EntryPointCandidate& right) {
            if (left.file == right.file) {
                return left.line < right.line;
            }
            return left.file < right.file;
        });

    std::unordered_map<std::string, ModuleSummary> modules;
    std::unordered_map<std::string, std::uint64_t> symbolCountsByFile;
    for (const SymbolInfo& symbol : result.symbols) {
        ++symbolCountsByFile[symbol.filePath.generic_string()];
    }

    for (const FileInfo& file : result.files) {
        std::string modulePath = ".";
        auto iterator = file.relativePath.begin();
        if (iterator != file.relativePath.end()) {
            modulePath = iterator->string();
            ++iterator;
            if (iterator != file.relativePath.end()) {
                modulePath += "/";
                modulePath += iterator->string();
            }
        }

        ModuleSummary& module = modules[modulePath];
        module.path = modulePath;
        ++module.fileCount;
        module.codeLines += file.lines.code;
        module.symbolCount += symbolCountsByFile[file.relativePath.generic_string()];
        if (module.sampleSymbol.empty()) {
            for (const SymbolInfo& symbol : file.symbols) {
                if (symbol.kind == "class" || symbol.kind == "struct" || symbol.kind == "function") {
                    module.sampleSymbol = symbol.name;
                    break;
                }
            }
        }
    }

    for (auto& [_, module] : modules) {
        overview.modules.push_back(module);
    }
    std::sort(
        overview.modules.begin(),
        overview.modules.end(),
        [](const ModuleSummary& left, const ModuleSummary& right) {
            if (left.codeLines == right.codeLines) {
                return left.path < right.path;
            }
            return left.codeLines > right.codeLines;
        });
    if (overview.modules.size() > 30) {
        overview.modules.resize(30);
    }

    std::unordered_map<std::string, std::uint64_t> includeFanIn;
    std::unordered_map<std::string, std::string> includeSample;
    for (const IncludeInfo& include : result.includes) {
        if (include.kind != "local") {
            continue;
        }
        const std::string key = basenameTarget(include.target);
        ++includeFanIn[key];
        if (includeSample[key].empty()) {
            includeSample[key] = include.filePath.generic_string();
        }
    }

    for (const FileInfo& file : result.files) {
        HotspotSummary hotspot;
        hotspot.path = file.relativePath.generic_string();
        hotspot.codeLines = file.lines.code;
        hotspot.totalLines = file.lines.total;
        hotspot.symbolCount = symbolCountsByFile[hotspot.path];
        hotspot.includeFanIn = includeFanIn[basenameTarget(file.relativePath.filename().string())];
        overview.hotspots.push_back(std::move(hotspot));
    }
    std::sort(
        overview.hotspots.begin(),
        overview.hotspots.end(),
        [](const HotspotSummary& left, const HotspotSummary& right) {
            if (left.codeLines == right.codeLines) {
                return left.path < right.path;
            }
            return left.codeLines > right.codeLines;
        });
    if (overview.hotspots.size() > 15) {
        overview.hotspots.resize(15);
    }

    for (const auto& [target, fanIn] : includeFanIn) {
        IncludeSketchItem item;
        item.target = target;
        item.fanIn = fanIn;
        item.sampleFile = includeSample[target];
        overview.includeSketch.push_back(std::move(item));
    }
    std::sort(
        overview.includeSketch.begin(),
        overview.includeSketch.end(),
        [](const IncludeSketchItem& left, const IncludeSketchItem& right) {
            if (left.fanIn == right.fanIn) {
                return left.target < right.target;
            }
            return left.fanIn > right.fanIn;
        });
    if (overview.includeSketch.size() > 15) {
        overview.includeSketch.resize(15);
    }

    overview.suggestedQuestions = {
        "可能的程序入口在哪？",
        "代码行最多的模块分别负责什么？",
        "谁 include 了被引用最多的头文件？",
        "顶层目录各自大概干什么？"
    };

    if (!overview.modules.empty()) {
        overview.suggestedQuestions.push_back(
            "`" + overview.modules.front().path + "` 目录大概负责什么？");
    }

    return overview;
}

void OverviewWriter::writeMarkdown(const ScanResult& result, std::ostream& output) const {
    const OverviewData overview = build(result);

    output << "# Project Overview — " << result.rootPath.string() << "\n\n";
    output << "Mode: `" << result.mode << "`  \n";
    output << "Confidence: `" << result.confidence << "`  \n";
    output << "Scan duration: " << result.scanDurationMs << " ms  \n";
    if (!result.compileCommandsPath.empty()) {
        output << "compile_commands: `" << result.compileCommandsPath.string()
               << "` (" << result.compileCommandsEntries << " entries)  \n";
    }
    output << "\n";

    output << "## 1. Snapshot\n\n";
    output << "- Files: " << result.files.size() << "\n";
    output << "- Source / header / inline: "
           << result.stats.sourceFiles << " / "
           << result.stats.headerFiles << " / "
           << result.stats.inlineFiles << "\n";
    output << "- Code lines: " << result.stats.lines.code
           << " (total " << result.stats.lines.total << ")\n";
    output << "- Symbols / includes / calls: "
           << result.symbols.size() << " / "
           << result.includes.size() << " / "
           << result.calls.size() << "\n";
    output << "- Reused from cache: " << result.reusedFileCount << "\n";
    output << "- Skipped entries: " << result.skippedEntries << "\n\n";

    output << "## 2. Module Map\n\n";
    output << "| Module | Files | Code lines | Symbols | Sample |\n";
    output << "| --- | ---: | ---: | ---: | --- |\n";
    for (const ModuleSummary& module : overview.modules) {
        output << "| `" << module.path << "` | "
               << module.fileCount << " | "
               << module.codeLines << " | "
               << module.symbolCount << " | "
               << module.sampleSymbol << " |\n";
    }
    output << "\n";

    output << "## 3. Entry Point Candidates\n\n";
    if (overview.entryPoints.empty()) {
        output << "_No common entry symbols found (`main` / `WinMain` / `DllMain` / ...)._\n\n";
    } else {
        output << "| Symbol | Location | Why | Confidence |\n";
        output << "| --- | --- | --- | --- |\n";
        for (const EntryPointCandidate& entry : overview.entryPoints) {
            output << "| `" << entry.name << "` | `"
                   << entry.file << ":" << entry.line << "` | "
                   << entry.reason << " | `"
                   << entry.confidence << "` |\n";
        }
        output << "\n";
    }

    output << "## 4. Hotspots\n\n";
    output << "| File | Code lines | Symbols | Include fan-in |\n";
    output << "| --- | ---: | ---: | ---: |\n";
    for (const HotspotSummary& hotspot : overview.hotspots) {
        output << "| `" << hotspot.path << "` | "
               << hotspot.codeLines << " | "
               << hotspot.symbolCount << " | "
               << hotspot.includeFanIn << " |\n";
    }
    output << "\n";

    output << "## 5. Include Sketch\n\n";
    output << "| Header | Fan-in | Sample includer |\n";
    output << "| --- | ---: | --- |\n";
    for (const IncludeSketchItem& item : overview.includeSketch) {
        output << "| `" << item.target << "` | "
               << item.fanIn << " | `"
               << item.sampleFile << "` |\n";
    }
    output << "\n";

    output << "## 6. How to dig deeper\n\n";
    for (const std::string& question : overview.suggestedQuestions) {
        output << "- " << question << "\n";
    }
    output << "\n";

    output << "## 7. Limits\n\n";
    output << "- This overview is based on heuristic parsing"
           << (result.mode == "hybrid" ? " plus compile_commands enhancement" : "")
           << ".\n";
    output << "- Without a complete build database, symbols/calls may be incomplete or noisy.\n";
    output << "- Treat entry points and module roles as investigation leads, not ground truth.\n";
    if (!result.diagnostics.empty()) {
        output << "\n### Diagnostics (first 20)\n\n";
        const std::size_t limit = std::min<std::size_t>(20, result.diagnostics.size());
        for (std::size_t index = 0; index < limit; ++index) {
            output << "- " << result.diagnostics[index] << "\n";
        }
    }
}

void OverviewWriter::writeMarkdownFile(
    const ScanResult& result,
    const std::filesystem::path& outputPath) const {
    std::filesystem::create_directories(outputPath.parent_path());
    std::ofstream output(outputPath);
    if (!output) {
        throw std::runtime_error("Failed to open overview output file: " + outputPath.string());
    }
    writeMarkdown(result, output);
}

void OverviewWriter::printTerminal(const ScanResult& result, std::ostream& output) const {
    const OverviewData overview = build(result);
    output << "Overview mode=" << result.mode
           << " files=" << result.files.size()
           << " code_lines=" << result.stats.lines.code
           << " duration_ms=" << result.scanDurationMs << "\n";

    output << "Modules (top):\n";
    const std::size_t moduleLimit = std::min<std::size_t>(8, overview.modules.size());
    for (std::size_t index = 0; index < moduleLimit; ++index) {
        const ModuleSummary& module = overview.modules[index];
        output << "  " << module.path << " — "
               << module.fileCount << " files, "
               << module.codeLines << " code lines\n";
    }

    output << "Entry points:\n";
    if (overview.entryPoints.empty()) {
        output << "  (none detected)\n";
    } else {
        for (const EntryPointCandidate& entry : overview.entryPoints) {
            output << "  " << entry.name << " @ "
                   << entry.file << ":" << entry.line
                   << " [" << entry.confidence << "]\n";
        }
    }
}

std::string OverviewWriter::buildClientBrief(const ScanResult& result) const {
    const OverviewData overview = build(result);
    std::ostringstream output;
    output << "# CppInsight Client Brief\n\n";
    output << "Project: " << result.rootPath.string() << "\n\n";
    output << "## Scale\n";
    output << "- Indexed C/C++ files: " << result.files.size() << "\n";
    output << "- Code lines: " << result.stats.lines.code << "\n";
    output << "- Analysis mode: " << result.mode << "\n\n";

    output << "## Likely structure\n";
    const std::size_t moduleLimit = std::min<std::size_t>(6, overview.modules.size());
    for (std::size_t index = 0; index < moduleLimit; ++index) {
        const ModuleSummary& module = overview.modules[index];
        output << "- `" << module.path << "`: "
               << module.fileCount << " files, "
               << module.codeLines << " code lines";
        if (!module.sampleSymbol.empty()) {
            output << " (e.g. `" << module.sampleSymbol << "`)";
        }
        output << "\n";
    }
    output << "\n";

    output << "## Entry candidates\n";
    if (overview.entryPoints.empty()) {
        output << "- No standard entry symbol detected yet.\n";
    } else {
        for (const EntryPointCandidate& entry : overview.entryPoints) {
            output << "- `" << entry.name << "` in `"
                   << entry.file << ":" << entry.line << "`\n";
        }
    }
    output << "\n";

    output << "## Hot files to inspect first\n";
    const std::size_t hotspotLimit = std::min<std::size_t>(5, overview.hotspots.size());
    for (std::size_t index = 0; index < hotspotLimit; ++index) {
        const HotspotSummary& hotspot = overview.hotspots[index];
        output << "- `" << hotspot.path << "` ("
               << hotspot.codeLines << " code lines)\n";
    }
    output << "\n";

    output << "## Caveat\n";
    output << "Generated for onboarding/estimation. Validate before committing scope.\n";
    return output.str();
}

} // namespace cppinsight
