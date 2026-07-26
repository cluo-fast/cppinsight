#pragma once

#include <cstdint>
#include <filesystem>
#include <functional>
#include <string>
#include <vector>

namespace cppinsight {

struct ScanProgress {
    std::uint64_t filesDiscovered = 0;
    std::uint64_t filesAnalyzed = 0;
    std::uint64_t filesReused = 0;
    std::filesystem::path currentFile;
};

using ProgressCallback = std::function<void(const ScanProgress& progress)>;

struct ScanOptions {
    std::vector<std::string> extraSkipDirectories;
    std::filesystem::path compileCommandsPath;
    std::filesystem::path previousScanJson;
    bool showProgress = true;
    ProgressCallback onProgress;
};

} // namespace cppinsight
