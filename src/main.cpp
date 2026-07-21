#include "common/Logger.h"
#include "project/ProjectScanner.h"
#include "report/JsonReportWriter.h"

#include <algorithm>
#include <exception>
#include <filesystem>
#include <iostream>
#include <string>
#include <stdexcept>

namespace {

void printUsage() {
    std::cout
        << "CppInsight AI\n\n"
        << "Usage:\n"
        << "  cppinsight scan <project-path> [--json <output-file>]\n";
}

struct CommandLineOptions {
    std::filesystem::path projectPath;
    std::filesystem::path jsonOutputPath;
    bool hasJsonOutput = false;
};

CommandLineOptions parseScanOptions(int argc, char* argv[]) {
    if (argc < 3) {
        throw std::runtime_error("Missing project path.");
    }

    CommandLineOptions options;
    options.projectPath = argv[2];

    for (int index = 3; index < argc; ++index) {
        const std::string argument = argv[index];

        if (argument == "--json") {
            if (index + 1 >= argc) {
                throw std::runtime_error("Missing path after --json.");
            }

            options.jsonOutputPath = argv[++index];
            options.hasJsonOutput = true;
            continue;
        }

        throw std::runtime_error("Unknown argument: " + argument);
    }

    return options;
}

void printScanSummary(const cppinsight::ScanResult& result) {
    std::cout << "Project: " << result.rootPath.string() << '\n';
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
    std::cout << "Skipped entries: " << result.skippedEntries << "\n\n";

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
        std::cout << "\nDiagnostics:\n";
        for (const std::string& diagnostic : result.diagnostics) {
            std::cout << "  " << diagnostic << '\n';
        }
    }
}

int runScan(int argc, char* argv[]) {
    const CommandLineOptions options = parseScanOptions(argc, argv);

    cppinsight::Logger::info("Scanning project...");
    const cppinsight::ProjectScanner scanner;
    const cppinsight::ScanResult result = scanner.scan(options.projectPath);

    printScanSummary(result);

    if (options.hasJsonOutput) {
        const cppinsight::JsonReportWriter writer;
        writer.writeFile(result, options.jsonOutputPath);
        cppinsight::Logger::info("JSON report written to " + options.jsonOutputPath.string());
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

        printUsage();
        cppinsight::Logger::error("Unknown command: " + command);
        return 1;
    } catch (const std::exception& exception) {
        cppinsight::Logger::error(exception.what());
        return 1;
    }
}
