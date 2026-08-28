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
    std::string qualifiedName;
    std::string stableKey;
    std::string parentQualifiedName;
    std::string parentStableKey;
    std::string signature;
    std::string scope;
    bool isDefinition = false;
    std::string confidence = "heuristic";
};

struct CallInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string name;
    std::string targetQualifiedName;
    std::string callerQualifiedName;
    std::string callerStableKey;
    std::string confidence = "heuristic";
};

struct SymbolRelationInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string sourceQualifiedName;
    std::string sourceStableKey;
    std::string targetName;
    std::string kind;
    std::string confidence = "heuristic";
};

struct EntryPointInfo {
    std::filesystem::path filePath;
    std::uint64_t line = 0;
    std::string qualifiedName;
    std::string symbolStableKey;
    std::string kind;
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
    std::vector<SymbolRelationInfo> relations;
    std::vector<EntryPointInfo> entryPoints;
};

} // namespace cppinsight
