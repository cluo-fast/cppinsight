#pragma once

#include "project/FileInfo.h"

#include <cstdint>
#include <filesystem>
#include <map>
#include <string>
#include <vector>

namespace cppinsight {

struct ScanStats {
    std::uint64_t sourceFiles = 0;
    std::uint64_t headerFiles = 0;
    std::uint64_t inlineFiles = 0;
    std::uint64_t totalBytes = 0;
    LineMetrics lines;
};

struct ScanResult {
    std::filesystem::path rootPath;
    std::vector<FileInfo> files;
    std::vector<IncludeInfo> includes;
    std::vector<SymbolInfo> symbols;
    std::vector<CallInfo> calls;
    std::vector<std::string> diagnostics;
    ScanStats stats;
    std::map<std::string, std::uint64_t> extensionCounts;
    std::map<std::string, std::uint64_t> topLevelDirectoryCounts;
    std::uint64_t skippedEntries = 0;
};

class ProjectScanner {
public:
    ScanResult scan(const std::filesystem::path& rootPath) const;

private:
    static bool isCppFile(const std::filesystem::path& path);
    static bool shouldSkipDirectory(const std::filesystem::path& path);
    static std::string categorizeExtension(const std::string& extension);
    static std::string normalizeExtension(std::string extension);
};

} // namespace cppinsight
