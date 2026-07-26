#pragma once

#include "project/FileInfo.h"

#include <filesystem>
#include <string>
#include <unordered_set>
#include <vector>

namespace cppinsight {

struct CompileCommandEntry {
    std::filesystem::path file;
    std::filesystem::path directory;
    std::vector<std::filesystem::path> includePaths;
};

struct CompileCommandsDatabase {
    std::vector<CompileCommandEntry> entries;
    std::unordered_set<std::string> compiledFiles;
    std::vector<std::filesystem::path> allIncludePaths;
    std::vector<std::string> diagnostics;
};

class CompileCommandsLoader {
public:
    static CompileCommandsDatabase load(const std::filesystem::path& path);
    static std::filesystem::path findDefault(const std::filesystem::path& projectRoot);
};

class PreciseEnhancer {
public:
    static void enhance(
        const std::filesystem::path& projectRoot,
        const CompileCommandsDatabase& database,
        std::vector<IncludeInfo>& includes,
        std::vector<FileInfo>& files,
        std::vector<std::string>& diagnostics);
};

} // namespace cppinsight
