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
};

struct SymbolInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string kind;
    std::string name;
    std::string signature;
    std::string scope;
    bool isDefinition = false;
};

struct CallInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string name;
};

struct FileInfo {
    std::filesystem::path absolutePath;
    std::filesystem::path relativePath;
    std::string extension;
    std::string category;
    std::uintmax_t sizeBytes = 0;
    LineMetrics lines;
    std::vector<IncludeInfo> includes;
    std::vector<SymbolInfo> symbols;
    std::vector<CallInfo> calls;
};

} // namespace cppinsight
