#pragma once

#include <cstdint>
#include <filesystem>
#include <string>
#include <vector>

namespace cppinsight {

struct LineMetrics {
    std::uint64_t total = 0;
    std::uint64_t blank = 0;
    std::uint64_t comment = 0;
    std::uint64_t code = 0;
};

struct IncludeInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string target;
    std::string kind;
    std::string resolvedPath;
    bool resolved = false;
};

struct SymbolInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string kind;
    std::string name;
    std::string signature;
    std::string scope;
    bool isDefinition = false;
    std::string confidence = "heuristic";
};

struct CallInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string name;
    std::string confidence = "heuristic";
};

struct FileInfo {
    std::filesystem::path absolutePath;
    std::filesystem::path relativePath;
    std::string extension;
    std::string category;
    std::uintmax_t sizeBytes = 0;
    std::uint64_t mtimeUnix = 0;
    bool fromCompileCommands = false;
    bool reusedFromCache = false;
    LineMetrics lines;
    std::vector<IncludeInfo> includes;
    std::vector<SymbolInfo> symbols;
    std::vector<CallInfo> calls;
};

} // namespace cppinsight
