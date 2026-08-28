#include "report/JsonReportWriter.h"

#include <fstream>
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
                   << "\"kind\": \"" << escapeJson(include.kind) << "\", "
                   << "\"resolved\": " << (include.resolved ? "true" : "false") << ", "
                   << "\"resolved_path\": \"" << escapeJson(include.resolvedPath) << "\""
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
                   << "\"qualified_name\": \"" << escapeJson(symbol.qualifiedName) << "\", "
                   << "\"stable_key\": \"" << escapeJson(symbol.stableKey) << "\", "
                   << "\"parent_qualified_name\": \"" << escapeJson(symbol.parentQualifiedName) << "\", "
                   << "\"parent_stable_key\": \"" << escapeJson(symbol.parentStableKey) << "\", "
                   << "\"signature\": \"" << escapeJson(symbol.signature) << "\", "
                   << "\"scope\": \"" << escapeJson(symbol.scope) << "\", "
                   << "\"is_definition\": " << (symbol.isDefinition ? "true" : "false") << ", "
                   << "\"confidence\": \"" << escapeJson(symbol.confidence) << "\""
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
                   << "\"name\": \"" << escapeJson(call.name) << "\", "
                   << "\"target_qualified_name\": \"" << escapeJson(call.targetQualifiedName) << "\", "
                   << "\"caller_qualified_name\": \"" << escapeJson(call.callerQualifiedName) << "\", "
                   << "\"caller_stable_key\": \"" << escapeJson(call.callerStableKey) << "\", "
                   << "\"confidence\": \"" << escapeJson(call.confidence) << "\""
                   << "}";
            if (index + 1 < calls.size()) {
                output << ",";
            }
            output << "\n";
        }
    };

    auto writeRelations = [&output](
                              const std::vector<SymbolRelationInfo>& relations,
                              const std::string& indent) {
        for (std::size_t index = 0; index < relations.size(); ++index) {
            const SymbolRelationInfo& relation = relations[index];
            output << indent << "{"
                   << "\"file\": \"" << escapeJson(relation.filePath.generic_string()) << "\", "
                   << "\"line\": " << relation.line << ", "
                   << "\"source_qualified_name\": \"" << escapeJson(relation.sourceQualifiedName) << "\", "
                   << "\"source_stable_key\": \"" << escapeJson(relation.sourceStableKey) << "\", "
                   << "\"target_name\": \"" << escapeJson(relation.targetName) << "\", "
                   << "\"kind\": \"" << escapeJson(relation.kind) << "\", "
                   << "\"confidence\": \"" << escapeJson(relation.confidence) << "\""
                   << "}";
            if (index + 1 < relations.size()) {
                output << ",";
            }
            output << "\n";
        }
    };

    auto writeEntryPoints = [&output](
                               const std::vector<EntryPointInfo>& entryPoints,
                               const std::string& indent) {
        for (std::size_t index = 0; index < entryPoints.size(); ++index) {
            const EntryPointInfo& entry = entryPoints[index];
            output << indent << "{"
                   << "\"file\": \"" << escapeJson(entry.filePath.generic_string()) << "\", "
                   << "\"line\": " << entry.line << ", "
                   << "\"qualified_name\": \"" << escapeJson(entry.qualifiedName) << "\", "
                   << "\"symbol_stable_key\": \"" << escapeJson(entry.symbolStableKey) << "\", "
                   << "\"kind\": \"" << escapeJson(entry.kind) << "\", "
                   << "\"confidence\": \"" << escapeJson(entry.confidence) << "\""
                   << "}";
            if (index + 1 < entryPoints.size()) {
                output << ",";
            }
            output << "\n";
        }
    };

    auto writeCounts = [&output](const std::map<std::string, std::uint64_t>& counts) {
        for (auto iterator = counts.begin(); iterator != counts.end(); ++iterator) {
            output << "    \"" << escapeJson(iterator->first) << "\": " << iterator->second;
            if (std::next(iterator) != counts.end()) {
                output << ",";
            }
            output << "\n";
        }
    };

    output << "{\n";
    output << "  \"schema_version\": 3,\n";
    output << "  \"generator_version\": \"cppinsight-core/0.1.0\",\n";
    output << "  \"root\": \"" << escapeJson(result.rootPath.string()) << "\",\n";
    output << "  \"mode\": \"" << escapeJson(result.mode) << "\",\n";
    output << "  \"confidence\": \"" << escapeJson(result.confidence) << "\",\n";
    output << "  \"scan_duration_ms\": " << result.scanDurationMs << ",\n";
    output << "  \"compile_commands\": \"" << escapeJson(result.compileCommandsPath.string()) << "\",\n";
    output << "  \"compile_commands_entries\": " << result.compileCommandsEntries << ",\n";
    output << "  \"reused_file_count\": " << result.reusedFileCount << ",\n";
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
    writeCounts(result.extensionCounts);
    output << "  },\n";
    output << "  \"top_level_layout\": {\n";
    writeCounts(result.topLevelDirectoryCounts);
    output << "  },\n";
    output << "  \"second_level_layout\": {\n";
    writeCounts(result.secondLevelDirectoryCounts);
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
    output << "  \"relations\": [\n";
    writeRelations(result.relations, "    ");
    output << "  ],\n";
    output << "  \"entry_points\": [\n";
    writeEntryPoints(result.entryPoints, "    ");
    output << "  ],\n";
    output << "  \"files\": [\n";

    for (std::size_t index = 0; index < result.files.size(); ++index) {
        const FileInfo& file = result.files[index];
        output << "    {\n";
        output << "      \"path\": \"" << escapeJson(file.relativePath.generic_string()) << "\",\n";
        output << "      \"extension\": \"" << escapeJson(file.extension) << "\",\n";
        output << "      \"category\": \"" << escapeJson(file.category) << "\",\n";
        output << "      \"size_bytes\": " << file.sizeBytes << ",\n";
        output << "      \"mtime_unix\": " << file.mtimeUnix << ",\n";
        output << "      \"from_compile_commands\": " << (file.fromCompileCommands ? "true" : "false") << ",\n";
        output << "      \"reused_from_cache\": " << (file.reusedFromCache ? "true" : "false") << ",\n";
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
        output << "      ],\n";
        output << "      \"relations\": [\n";
        writeRelations(file.relations, "        ");
        output << "      ],\n";
        output << "      \"entry_points\": [\n";
        writeEntryPoints(file.entryPoints, "        ");
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
    if (!outputPath.parent_path().empty()) {
        std::filesystem::create_directories(outputPath.parent_path());
    }

    std::ofstream output(outputPath);
    if (!output) {
        throw std::runtime_error("Failed to open JSON output file: " + outputPath.string());
    }

    write(result, output);
}

namespace {

void appendUnicodeEscape(std::ostringstream& output, unsigned int codeUnit) {
    static const char* const hex = "0123456789abcdef";
    output << "\\u"
           << hex[(codeUnit >> 12) & 0xF]
           << hex[(codeUnit >> 8) & 0xF]
           << hex[(codeUnit >> 4) & 0xF]
           << hex[codeUnit & 0xF];
}

int utf8SequenceLength(unsigned char lead) {
    if (lead < 0x80) {
        return 1;
    }
    if ((lead & 0xE0) == 0xC0) {
        return 2;
    }
    if ((lead & 0xF0) == 0xE0) {
        return 3;
    }
    if ((lead & 0xF8) == 0xF0) {
        return 4;
    }
    return -1;
}

bool isUtf8Continuation(unsigned char character) {
    return (character & 0xC0) == 0x80;
}

bool isValidUtf8Sequence(const unsigned char* bytes, int length) {
    if (length <= 0) {
        return false;
    }
    for (int index = 1; index < length; ++index) {
        if (!isUtf8Continuation(bytes[index])) {
            return false;
        }
    }
    // Reject overlong / invalid lead ranges used by Windows local encodings mistaken as UTF-8.
    if (length == 2 && bytes[0] < 0xC2) {
        return false;
    }
    if (length == 3 && bytes[0] == 0xE0 && bytes[1] < 0xA0) {
        return false;
    }
    if (length == 4 && bytes[0] == 0xF0 && bytes[1] < 0x90) {
        return false;
    }
    if (length == 4 && bytes[0] > 0xF4) {
        return false;
    }
    return true;
}

} // namespace

std::string JsonReportWriter::escapeJson(const std::string& value) {
    std::ostringstream escaped;
    const auto* bytes = reinterpret_cast<const unsigned char*>(value.data());
    const std::size_t size = value.size();

    for (std::size_t index = 0; index < size;) {
        const unsigned char character = bytes[index];
        switch (character) {
        case '\"':
            escaped << "\\\"";
            ++index;
            break;
        case '\\':
            escaped << "\\\\";
            ++index;
            break;
        case '\b':
            escaped << "\\b";
            ++index;
            break;
        case '\f':
            escaped << "\\f";
            ++index;
            break;
        case '\n':
            escaped << "\\n";
            ++index;
            break;
        case '\r':
            escaped << "\\r";
            ++index;
            break;
        case '\t':
            escaped << "\\t";
            ++index;
            break;
        default:
            if (character < 0x20) {
                appendUnicodeEscape(escaped, character);
                ++index;
                break;
            }
            if (character < 0x80) {
                escaped << static_cast<char>(character);
                ++index;
                break;
            }

            const int length = utf8SequenceLength(character);
            if (length > 0 && index + static_cast<std::size_t>(length) <= size &&
                isValidUtf8Sequence(bytes + index, length)) {
                for (int offset = 0; offset < length; ++offset) {
                    escaped << static_cast<char>(bytes[index + static_cast<std::size_t>(offset)]);
                }
                index += static_cast<std::size_t>(length);
            } else {
                // Local encodings (e.g. GBK paths/signatures) must not leak into the JSON file.
                appendUnicodeEscape(escaped, character);
                ++index;
            }
            break;
        }
    }

    return escaped.str();
}

} // namespace cppinsight
