#pragma once

#include "project/ProjectScanner.h"

#include <filesystem>
#include <ostream>
#include <string>
#include <vector>

namespace cppinsight {

struct EntryPointCandidate {
    std::string name;
    std::string file;
    std::uint64_t line = 0;
    std::string reason;
    std::string confidence;
};

struct ModuleSummary {
    std::string path;
    std::uint64_t fileCount = 0;
    std::uint64_t codeLines = 0;
    std::uint64_t symbolCount = 0;
    std::string sampleSymbol;
};

struct HotspotSummary {
    std::string path;
    std::uint64_t codeLines = 0;
    std::uint64_t totalLines = 0;
    std::uint64_t symbolCount = 0;
    std::uint64_t includeFanIn = 0;
};

struct IncludeSketchItem {
    std::string target;
    std::uint64_t fanIn = 0;
    std::string sampleFile;
};

struct OverviewData {
    std::vector<EntryPointCandidate> entryPoints;
    std::vector<ModuleSummary> modules;
    std::vector<HotspotSummary> hotspots;
    std::vector<IncludeSketchItem> includeSketch;
    std::vector<std::string> suggestedQuestions;
};

class OverviewWriter {
public:
    OverviewData build(const ScanResult& result) const;
    void writeMarkdown(const ScanResult& result, std::ostream& output) const;
    void writeMarkdownFile(const ScanResult& result, const std::filesystem::path& outputPath) const;
    void printTerminal(const ScanResult& result, std::ostream& output) const;
    std::string buildClientBrief(const ScanResult& result) const;

private:
    static bool isEntryPointName(const std::string& name);
};

} // namespace cppinsight
