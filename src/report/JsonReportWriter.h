#pragma once

#include "project/ProjectScanner.h"

#include <filesystem>
#include <ostream>

namespace cppinsight {

class JsonReportWriter {
public:
    void write(const ScanResult& result, std::ostream& output) const;
    void writeFile(const ScanResult& result, const std::filesystem::path& outputPath) const;

private:
    static std::string escapeJson(const std::string& value);
};

} // namespace cppinsight
