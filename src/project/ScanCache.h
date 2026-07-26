#pragma once

#include "project/FileInfo.h"
#include "project/ProjectScanner.h"

#include <filesystem>
#include <unordered_map>

namespace cppinsight {

class ScanCache {
public:
    static std::unordered_map<std::string, FileInfo> loadPreviousFiles(
        const std::filesystem::path& previousScanJson,
        std::vector<std::string>& diagnostics);

    static bool isReusable(
        const FileInfo& cached,
        std::uintmax_t sizeBytes,
        std::uint64_t mtimeUnix);
};

} // namespace cppinsight
