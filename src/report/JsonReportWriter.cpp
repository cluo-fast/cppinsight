#include "report/JsonReportWriter.h"

#include <fstream>
#include <iomanip>
#include <iterator>
#include <sstream>
#include <stdexcept>

namespace cppinsight {

void JsonReportWriter::write(const ScanResult& result, std::ostream& output) const {
    auto writeDiagnostics = [&output](const std::vector<std::string>& diagnostics, const std::string& indent) {
        for (std::size_t index = 0; index < diagnostics.size(); ++index) {
            output << indent << "\"" << escapeJson(diagnostics[index]) << "\"";
            if (index + 1 < diagnostics.size()) {
                output << ",";
            }
            output << "\n";
        }
    };

    auto writeIncludes = [&output](const std::vector<IncludeInfo>& includes, const std::string& indent) {
        for (std::size_t index = 0; index < includes.size(); ++index) {
            const IncludeInfo& include = includes[index];
            output << indent << "{"
                   << "\"file\": \"" << escapeJson(include.filePath.generic_string()) << "\", "
                   << "\"line\": " << include.line << ", "
                   << "\"target\": \"" << escapeJson(include.target) << "\", "
                   << "\"kind\": \"" << escapeJson(include.kind) << "\""
                   << "}";
            if (index + 1 < includes.size()) {
                output << ",";
            }
            output << "\n";
        }
    };

    auto writeSymbols = [&output](const std::vector<SymbolInfo>& symbols, const std::string& indent) {
        for (std::size_t index = 0; index < symbols.size(); ++index) {
            const SymbolInfo& symbol = symbols[index];
            output << indent << "{"
                   << "\"file\": \"" << escapeJson(symbol.filePath.generic_string()) << "\", "
                   << "\"line\": " << symbol.line << ", "
                   << "\"kind\": \"" << escapeJson(symbol.kind) << "\", "
                   << "\"name\": \"" << escapeJson(symbol.name) << "\", "
                   << "\"signature\": \"" << escapeJson(symbol.signature) << "\", "
                   << "\"scope\": \"" << escapeJson(symbol.scope) << "\", "
                   << "\"is_definition\": " << (symbol.isDefinition ? "true" : "false")
                   << "}";
            if (index + 1 < symbols.size()) {
                output << ",";
            }
            output << "\n";
        }
    };

    auto writeCalls = [&output](const std::vector<CallInfo>& calls, const std::string& indent) {
        for (std::size_t index = 0; index < calls.size(); ++index) {
            const CallInfo& call = calls[index];
            output << indent << "{"
                   << "\"file\": \"" << escapeJson(call.filePath.generic_string()) << "\", "
                   << "\"line\": " << call.line << ", "
                   << "\"name\": \"" << escapeJson(call.name) << "\""
                   << "}";
            if (index + 1 < calls.size()) {
                output << ",";
            }
            output << "\n";
        }
    };

    output << "{\n";
    output << "  \"schema_version\": 1,\n";
    output << "  \"root\": \"" << escapeJson(result.rootPath.string()) << "\",\n";
    output << "  \"file_count\": " << result.files.size() << ",\n";
    output << "  \"skipped_entries\": " << result.skippedEntries << ",\n";
    output << "  \"stats\": {\n";
    output << "    \"source_files\": " << result.stats.sourceFiles << ",\n";
    output << "    \"header_files\": " << result.stats.headerFiles << ",\n";
    output << "    \"inline_files\": " << result.stats.inlineFiles << ",\n";
    output << "    \"total_bytes\": " << result.stats.totalBytes << ",\n";
    output << "    \"lines\": {\n";
    output << "      \"total\": " << result.stats.lines.total << ",\n";
    output << "      \"code\": " << result.stats.lines.code << ",\n";
    output << "      \"comment\": " << result.stats.lines.comment << ",\n";
    output << "      \"blank\": " << result.stats.lines.blank << "\n";
    output << "    }\n";
    output << "  },\n";
    output << "  \"extensions\": {\n";
    for (auto iterator = result.extensionCounts.begin(); iterator != result.extensionCounts.end(); ++iterator) {
        output << "    \"" << escapeJson(iterator->first) << "\": " << iterator->second;
        if (std::next(iterator) != result.extensionCounts.end()) {
            output << ",";
        }
        output << "\n";
    }
    output << "  },\n";
    output << "  \"top_level_layout\": {\n";
    for (auto iterator = result.topLevelDirectoryCounts.begin();
         iterator != result.topLevelDirectoryCounts.end();
         ++iterator) {
        output << "    \"" << escapeJson(iterator->first) << "\": " << iterator->second;
        if (std::next(iterator) != result.topLevelDirectoryCounts.end()) {
            output << ",";
        }
        output << "\n";
    }
    output << "  },\n";
    output << "  \"diagnostics\": [\n";
    writeDiagnostics(result.diagnostics, "    ");
    output << "  ],\n";
    output << "  \"includes\": [\n";
    writeIncludes(result.includes, "    ");
    output << "  ],\n";
    output << "  \"symbols\": [\n";
    writeSymbols(result.symbols, "    ");
    output << "  ],\n";
    output << "  \"calls\": [\n";
    writeCalls(result.calls, "    ");
    output << "  ],\n";
    output << "  \"files\": [\n";

    for (std::size_t index = 0; index < result.files.size(); ++index) {
        const FileInfo& file = result.files[index];
        output << "    {\n";
        output << "      \"path\": \"" << escapeJson(file.relativePath.generic_string()) << "\",\n";
        output << "      \"extension\": \"" << escapeJson(file.extension) << "\",\n";
        output << "      \"category\": \"" << escapeJson(file.category) << "\",\n";
        output << "      \"size_bytes\": " << file.sizeBytes << ",\n";
        output << "      \"lines\": {\n";
        output << "        \"total\": " << file.lines.total << ",\n";
        output << "        \"code\": " << file.lines.code << ",\n";
        output << "        \"comment\": " << file.lines.comment << ",\n";
        output << "        \"blank\": " << file.lines.blank << "\n";
        output << "      },\n";
        output << "      \"includes\": [\n";
        writeIncludes(file.includes, "        ");
        output << "      ],\n";
        output << "      \"symbols\": [\n";
        writeSymbols(file.symbols, "        ");
        output << "      ],\n";
        output << "      \"calls\": [\n";
        writeCalls(file.calls, "        ");
        output << "      ]\n";
        output << "    }";

        if (index + 1 < result.files.size()) {
            output << ",";
        }

        output << "\n";
    }

    output << "  ]\n";
    output << "}\n";
}

void JsonReportWriter::writeFile(const ScanResult& result, const std::filesystem::path& outputPath) const {
    std::ofstream output(outputPath);
    if (!output) {
        throw std::runtime_error("Failed to open JSON output file: " + outputPath.string());
    }

    write(result, output);
}

std::string JsonReportWriter::escapeJson(const std::string& value) {
    std::ostringstream escaped;

    for (const char character : value) {
        switch (character) {
        case '\"':
            escaped << "\\\"";
            break;
        case '\\':
            escaped << "\\\\";
            break;
        case '\b':
            escaped << "\\b";
            break;
        case '\f':
            escaped << "\\f";
            break;
        case '\n':
            escaped << "\\n";
            break;
        case '\r':
            escaped << "\\r";
            break;
        case '\t':
            escaped << "\\t";
            break;
        default:
            if (static_cast<unsigned char>(character) < 0x20) {
                escaped << "\\u"
                        << std::hex << std::setw(4) << std::setfill('0')
                        << static_cast<int>(static_cast<unsigned char>(character));
            } else {
                escaped << character;
            }
            break;
        }
    }

    return escaped.str();
}

} // namespace cppinsight
