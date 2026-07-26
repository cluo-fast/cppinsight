#include "common/Logger.h"
#include "project/ProjectScanner.h"
#include "project/ScanOptions.h"
#include "report/JsonReportWriter.h"
#include "report/OverviewWriter.h"

#include <algorithm>
#include <exception>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <stdexcept>
#include <vector>

namespace {

void printUsage() {
    std::cout
        << "CppInsight AI\n\n"
        << "Usage:\n"
        << "  cppinsight scan <project-path> [options]\n"
        << "  cppinsight overview <project-path> [options]\n"
        << "  cppinsight brief <project-path> [--output <file>]\n\n"
        << "Scan/overview options:\n"
        << "  --json <output-file>              Write inventory JSON\n"
        << "  --overview <output-file>          Write Markdown overview\n"
        << "  --compile-commands <file>         Use compile_commands.json (hybrid mode)\n"
        << "  --incremental [previous-json]     Reuse unchanged files from previous scan\n"
        << "  --skip-dir <name>                 Extra directory name to skip (repeatable)\n"
        << "  --quiet                           Suppress scan progress\n";
}

struct CommandLineOptions {
    std::filesystem::path projectPath;
    std::filesystem::path jsonOutputPath;
    std::filesystem::path overviewOutputPath;
    std::filesystem::path briefOutputPath;
    std::filesystem::path compileCommandsPath;
    std::filesystem::path previousScanJson;
    std::vector<std::string> extraSkipDirectories;
    bool hasJsonOutput = false;
    bool hasOverviewOutput = false;
    bool hasBriefOutput = false;
    bool incremental = false;
    bool quiet = false;
};

CommandLineOptions parseOptions(int argc, char* argv[], bool requireProjectPath) {
    if (requireProjectPath && argc < 3) {
        throw std::runtime_error("Missing project path.");
    }

    CommandLineOptions options;
    if (requireProjectPath) {
        options.projectPath = argv[2];
    }

    for (int index = requireProjectPath ? 3 : 2; index < argc; ++index) {
        const std::string argument = argv[index];

        auto requireValue = [&](const char* name) -> std::filesystem::path {
            if (index + 1 >= argc) {
                throw std::runtime_error(std::string("Missing path after ") + name + ".");
            }
            return argv[++index];
        };

        if (argument == "--json") {
            options.jsonOutputPath = requireValue("--json");
            options.hasJsonOutput = true;
            continue;
        }
        if (argument == "--overview") {
            options.overviewOutputPath = requireValue("--overview");
            options.hasOverviewOutput = true;
            continue;
        }
        if (argument == "--output" || argument == "-o") {
            options.overviewOutputPath = requireValue(argument.c_str());
            options.hasOverviewOutput = true;
            options.briefOutputPath = options.overviewOutputPath;
            options.hasBriefOutput = true;
            continue;
        }
        if (argument == "--compile-commands") {
            options.compileCommandsPath = requireValue("--compile-commands");
            continue;
        }
        if (argument == "--incremental") {
            options.incremental = true;
            if (index + 1 < argc) {
                const std::string next = argv[index + 1];
                if (!next.empty() && next[0] != '-') {
                    options.previousScanJson = argv[++index];
                }
            }
            continue;
        }
        if (argument == "--skip-dir") {
            if (index + 1 >= argc) {
                throw std::runtime_error("Missing directory name after --skip-dir.");
            }
            options.extraSkipDirectories.push_back(argv[++index]);
            continue;
        }
        if (argument == "--quiet") {
            options.quiet = true;
            continue;
        }

        throw std::runtime_error("Unknown argument: " + argument);
    }

    if (options.incremental && options.previousScanJson.empty()) {
        const std::filesystem::path defaultPrevious =
            options.projectPath / ".cppinsight" / "scan.json";
        if (std::filesystem::exists(defaultPrevious)) {
            options.previousScanJson = defaultPrevious;
        } else if (options.hasJsonOutput && std::filesystem::exists(options.jsonOutputPath)) {
            options.previousScanJson = options.jsonOutputPath;
        }
    }

    return options;
}

cppinsight::ScanOptions toScanOptions(const CommandLineOptions& options) {
    cppinsight::ScanOptions scanOptions;
    scanOptions.extraSkipDirectories = options.extraSkipDirectories;
    scanOptions.compileCommandsPath = options.compileCommandsPath;
    scanOptions.previousScanJson = options.previousScanJson;
    scanOptions.showProgress = !options.quiet;
    if (!options.quiet) {
        scanOptions.onProgress = [](const cppinsight::ScanProgress& progress) {
            if (progress.filesDiscovered == 1 || progress.filesDiscovered % 200 == 0) {
                std::cerr << "[scan] files=" << progress.filesDiscovered
                          << " analyzed=" << progress.filesAnalyzed
                          << " reused=" << progress.filesReused
                          << " current=" << progress.currentFile.generic_string() << '\n';
            }
        };
    }
    return scanOptions;
}

void printScanSummary(const cppinsight::ScanResult& result) {
    std::cout << "Project: " << result.rootPath.string() << '\n';
    std::cout << "Mode: " << result.mode << " (" << result.confidence << ")\n";
    std::cout << "Duration: " << result.scanDurationMs << " ms\n";
    std::cout << "C/C++ files: " << result.files.size() << '\n';
    std::cout << "  Source files: " << result.stats.sourceFiles << '\n';
    std::cout << "  Header files: " << result.stats.headerFiles << '\n';
    std::cout << "  Inline files: " << result.stats.inlineFiles << '\n';
    std::cout << "Total lines: " << result.stats.lines.total << '\n';
    std::cout << "  Code lines: " << result.stats.lines.code << '\n';
    std::cout << "  Comment lines: " << result.stats.lines.comment << '\n';
    std::cout << "  Blank lines: " << result.stats.lines.blank << '\n';
    std::cout << "Total bytes: " << result.stats.totalBytes << "\n\n";
    std::cout << "Includes: " << result.includes.size() << '\n';
    std::cout << "Symbols: " << result.symbols.size() << '\n';
    std::cout << "Call clues: " << result.calls.size() << '\n';
    std::cout << "Reused files: " << result.reusedFileCount << '\n';
    std::cout << "Skipped entries: " << result.skippedEntries << "\n\n";

    if (!result.compileCommandsPath.empty()) {
        std::cout << "compile_commands: " << result.compileCommandsPath.string()
                  << " (" << result.compileCommandsEntries << " entries)\n\n";
    }

    if (!result.extensionCounts.empty()) {
        std::cout << "Extensions:\n";
        for (const auto& [extension, count] : result.extensionCounts) {
            std::cout << "  " << extension << ": " << count << '\n';
        }
        std::cout << '\n';
    }

    if (!result.topLevelDirectoryCounts.empty()) {
        std::cout << "Top-level layout:\n";
        for (const auto& [directory, count] : result.topLevelDirectoryCounts) {
            std::cout << "  " << directory << ": " << count << " files\n";
        }
        std::cout << '\n';
    }

    constexpr std::size_t maxPreviewFiles = 20;
    const std::size_t previewCount = std::min(maxPreviewFiles, result.files.size());

    std::cout << "Files:\n";
    for (std::size_t index = 0; index < previewCount; ++index) {
        const cppinsight::FileInfo& file = result.files[index];
        std::cout << "  " << file.relativePath.generic_string()
                  << " (" << file.lines.total << " lines, "
                  << file.category << ")\n";
    }

    if (result.files.size() > previewCount) {
        std::cout << "  ... " << (result.files.size() - previewCount) << " more files\n";
    }

    if (!result.diagnostics.empty()) {
        std::cout << "\nDiagnostics (first 10):\n";
        const std::size_t limit = std::min<std::size_t>(10, result.diagnostics.size());
        for (std::size_t index = 0; index < limit; ++index) {
            std::cout << "  " << result.diagnostics[index] << '\n';
        }
    }
}

cppinsight::ScanResult runScanner(const CommandLineOptions& options) {
    cppinsight::Logger::info("Scanning project...");
    const cppinsight::ProjectScanner scanner;
    return scanner.scan(options.projectPath, toScanOptions(options));
}

void writeArtifacts(const CommandLineOptions& options, const cppinsight::ScanResult& result) {
    if (options.hasJsonOutput) {
        const cppinsight::JsonReportWriter writer;
        writer.writeFile(result, options.jsonOutputPath);
        cppinsight::Logger::info("JSON report written to " + options.jsonOutputPath.string());
    }

    if (options.hasOverviewOutput) {
        try {
            const cppinsight::OverviewWriter writer;
            writer.writeMarkdownFile(result, options.overviewOutputPath);
            cppinsight::Logger::info("Overview written to " + options.overviewOutputPath.string());
        } catch (const std::exception& exception) {
            // Desktop indexing only needs JSON; do not fail the whole scan if overview I/O fails.
            cppinsight::Logger::warning(
                std::string("Overview write skipped: ") + exception.what());
        }
    }
}

int runScan(int argc, char* argv[]) {
    CommandLineOptions options = parseOptions(argc, argv, true);
    if (!options.hasJsonOutput) {
        options.jsonOutputPath = options.projectPath / ".cppinsight" / "scan.json";
        options.hasJsonOutput = true;
    }
    if (!options.hasOverviewOutput) {
        options.overviewOutputPath = options.projectPath / ".cppinsight" / "overview.md";
        options.hasOverviewOutput = true;
    }

    const cppinsight::ScanResult result = runScanner(options);
    printScanSummary(result);

    const cppinsight::OverviewWriter overviewWriter;
    overviewWriter.printTerminal(result, std::cout);
    std::cout << '\n';

    writeArtifacts(options, result);
    return 0;
}

int runOverview(int argc, char* argv[]) {
    CommandLineOptions options = parseOptions(argc, argv, true);
    if (!options.hasOverviewOutput) {
        options.overviewOutputPath = options.projectPath / ".cppinsight" / "overview.md";
        options.hasOverviewOutput = true;
    }

    const cppinsight::ScanResult result = runScanner(options);
    const cppinsight::OverviewWriter writer;
    writer.printTerminal(result, std::cout);
    writer.writeMarkdownFile(result, options.overviewOutputPath);
    cppinsight::Logger::info("Overview written to " + options.overviewOutputPath.string());

    if (options.hasJsonOutput) {
        const cppinsight::JsonReportWriter jsonWriter;
        jsonWriter.writeFile(result, options.jsonOutputPath);
    }

    return 0;
}

int runBrief(int argc, char* argv[]) {
    CommandLineOptions options = parseOptions(argc, argv, true);
    const cppinsight::ScanResult result = runScanner(options);
    const cppinsight::OverviewWriter writer;
    const std::string brief = writer.buildClientBrief(result);

    if (options.hasBriefOutput) {
        std::filesystem::create_directories(options.briefOutputPath.parent_path());
        std::ofstream output(options.briefOutputPath);
        if (!output) {
            throw std::runtime_error("Failed to write brief: " + options.briefOutputPath.string());
        }
        output << brief;
        cppinsight::Logger::info("Client brief written to " + options.briefOutputPath.string());
    } else {
        std::cout << brief;
    }

    return 0;
}

} // namespace

int main(int argc, char* argv[]) {
    try {
        if (argc < 2) {
            printUsage();
            return 1;
        }

        const std::string command = argv[1];
        if (command == "scan") {
            return runScan(argc, argv);
        }
        if (command == "overview") {
            return runOverview(argc, argv);
        }
        if (command == "brief") {
            return runBrief(argc, argv);
        }

        printUsage();
        cppinsight::Logger::error("Unknown command: " + command);
        return 1;
    } catch (const std::exception& exception) {
        cppinsight::Logger::error(exception.what());
        return 1;
    }
}
