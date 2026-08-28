#include "project/ScanCache.h"

#include <cctype>
#include <fstream>
#include <sstream>

namespace cppinsight {
namespace {

std::string readAll(const std::filesystem::path& path) {
    std::ifstream input(path, std::ios::binary);
    if (!input) {
        return {};
    }
    std::ostringstream buffer;
    buffer << input.rdbuf();
    return buffer.str();
}

std::string unescape(const std::string& value) {
    std::string output;
    output.reserve(value.size());
    for (std::size_t index = 0; index < value.size(); ++index) {
        if (value[index] != '\\' || index + 1 >= value.size()) {
            output.push_back(value[index]);
            continue;
        }
        const char next = value[++index];
        switch (next) {
        case '"':
        case '\\':
        case '/':
            output.push_back(next);
            break;
        case 'n':
            output.push_back('\n');
            break;
        case 'r':
            output.push_back('\r');
            break;
        case 't':
            output.push_back('\t');
            break;
        default:
            output.push_back(next);
            break;
        }
    }
    return output;
}

bool readStringField(const std::string& object, const std::string& name, std::string& value) {
    const std::string key = "\"" + name + "\"";
    std::size_t pos = object.find(key);
    if (pos == std::string::npos) {
        return false;
    }
    pos = object.find(':', pos + key.size());
    if (pos == std::string::npos) {
        return false;
    }
    pos = object.find('"', pos + 1);
    if (pos == std::string::npos) {
        return false;
    }
    std::size_t end = pos + 1;
    while (end < object.size()) {
        if (object[end] == '\\') {
            end += 2;
            continue;
        }
        if (object[end] == '"') {
            value = unescape(object.substr(pos + 1, end - pos - 1));
            return true;
        }
        ++end;
    }
    return false;
}

bool readUintField(const std::string& object, const std::string& name, std::uint64_t& value) {
    const std::string key = "\"" + name + "\"";
    std::size_t pos = object.find(key);
    if (pos == std::string::npos) {
        return false;
    }
    pos = object.find(':', pos + key.size());
    if (pos == std::string::npos) {
        return false;
    }
    ++pos;
    while (pos < object.size() && std::isspace(static_cast<unsigned char>(object[pos])) != 0) {
        ++pos;
    }
    std::size_t end = pos;
    while (end < object.size() && std::isdigit(static_cast<unsigned char>(object[end])) != 0) {
        ++end;
    }
    if (end == pos) {
        return false;
    }
    value = std::stoull(object.substr(pos, end - pos));
    return true;
}

bool readBoolField(const std::string& object, const std::string& name, bool& value) {
    const std::string key = "\"" + name + "\"";
    std::size_t pos = object.find(key);
    if (pos == std::string::npos) {
        return false;
    }
    pos = object.find(':', pos + key.size());
    if (pos == std::string::npos) {
        return false;
    }
    if (object.find("true", pos) == pos + 1 || object.find("true", pos) == pos + 2) {
        const std::size_t truePos = object.find("true", pos);
        if (truePos != std::string::npos && truePos < pos + 5) {
            value = true;
            return true;
        }
    }
    if (object.find("false", pos) != std::string::npos && object.find("false", pos) < pos + 8) {
        value = false;
        return true;
    }
    return false;
}

std::string extractArrayObject(const std::string& text, std::size_t& cursor) {
    const std::size_t begin = text.find('{', cursor);
    if (begin == std::string::npos) {
        cursor = text.size();
        return {};
    }

    int depth = 0;
    bool inString = false;
    bool escaped = false;
    for (std::size_t index = begin; index < text.size(); ++index) {
        const char character = text[index];
        if (inString) {
            if (escaped) {
                escaped = false;
            } else if (character == '\\') {
                escaped = true;
            } else if (character == '"') {
                inString = false;
            }
            continue;
        }

        if (character == '"') {
            inString = true;
            continue;
        }
        if (character == '{') {
            ++depth;
        } else if (character == '}') {
            --depth;
            if (depth == 0) {
                cursor = index + 1;
                return text.substr(begin, index - begin + 1);
            }
        }
    }

    cursor = text.size();
    return {};
}

LineMetrics readLineMetrics(const std::string& object) {
    LineMetrics lines;
    const std::size_t linesPos = object.find("\"lines\"");
    if (linesPos == std::string::npos) {
        return lines;
    }
    const std::string slice = object.substr(linesPos, std::min<std::size_t>(object.size() - linesPos, 200));
    readUintField(slice, "total", lines.total);
    readUintField(slice, "code", lines.code);
    readUintField(slice, "comment", lines.comment);
    readUintField(slice, "blank", lines.blank);
    return lines;
}

std::vector<IncludeInfo> readIncludes(const std::string& object, const std::filesystem::path& fallbackFile) {
    std::vector<IncludeInfo> includes;
    const std::size_t arrayPos = object.find("\"includes\"");
    if (arrayPos == std::string::npos) {
        return includes;
    }
    std::size_t cursor = object.find('[', arrayPos);
    if (cursor == std::string::npos) {
        return includes;
    }
    const std::size_t arrayEnd = object.find(']', cursor);
    if (arrayEnd == std::string::npos) {
        return includes;
    }
    const std::string arrayText = object.substr(cursor, arrayEnd - cursor + 1);
    cursor = 0;
    while (cursor < arrayText.size()) {
        const std::string item = extractArrayObject(arrayText, cursor);
        if (item.empty()) {
            break;
        }
        IncludeInfo include;
        std::string file;
        readStringField(item, "file", file);
        include.filePath = file.empty() ? fallbackFile : std::filesystem::path(file);
        readUintField(item, "line", include.line);
        readStringField(item, "target", include.target);
        readStringField(item, "kind", include.kind);
        readStringField(item, "resolved_path", include.resolvedPath);
        readBoolField(item, "resolved", include.resolved);
        includes.push_back(std::move(include));
    }
    return includes;
}

std::vector<SymbolInfo> readSymbols(const std::string& object, const std::filesystem::path& fallbackFile) {
    std::vector<SymbolInfo> symbols;
    const std::size_t arrayPos = object.find("\"symbols\"");
    if (arrayPos == std::string::npos) {
        return symbols;
    }
    std::size_t cursor = object.find('[', arrayPos);
    if (cursor == std::string::npos) {
        return symbols;
    }
    const std::size_t arrayEnd = object.find("],", cursor);
    const std::size_t arrayEndAlt = object.find(']', cursor);
    const std::size_t end = arrayEnd != std::string::npos ? arrayEnd : arrayEndAlt;
    if (end == std::string::npos) {
        return symbols;
    }
    const std::string arrayText = object.substr(cursor, end - cursor + 1);
    cursor = 0;
    while (cursor < arrayText.size()) {
        const std::string item = extractArrayObject(arrayText, cursor);
        if (item.empty()) {
            break;
        }
        SymbolInfo symbol;
        std::string file;
        readStringField(item, "file", file);
        symbol.filePath = file.empty() ? fallbackFile : std::filesystem::path(file);
        readUintField(item, "line", symbol.line);
        readStringField(item, "kind", symbol.kind);
        readStringField(item, "name", symbol.name);
        readStringField(item, "qualified_name", symbol.qualifiedName);
        readStringField(item, "stable_key", symbol.stableKey);
        readStringField(item, "parent_qualified_name", symbol.parentQualifiedName);
        readStringField(item, "parent_stable_key", symbol.parentStableKey);
        readStringField(item, "signature", symbol.signature);
        readStringField(item, "scope", symbol.scope);
        readBoolField(item, "is_definition", symbol.isDefinition);
        if (!readStringField(item, "confidence", symbol.confidence)) {
            symbol.confidence = "heuristic";
        }
        symbols.push_back(std::move(symbol));
    }
    return symbols;
}

std::vector<CallInfo> readCalls(const std::string& object, const std::filesystem::path& fallbackFile) {
    std::vector<CallInfo> calls;
    const std::size_t arrayPos = object.find("\"calls\"");
    if (arrayPos == std::string::npos) {
        return calls;
    }
    std::size_t cursor = object.find('[', arrayPos);
    if (cursor == std::string::npos) {
        return calls;
    }
    const std::size_t arrayEnd = object.find(']', cursor);
    if (arrayEnd == std::string::npos) {
        return calls;
    }
    const std::string arrayText = object.substr(cursor, arrayEnd - cursor + 1);
    cursor = 0;
    while (cursor < arrayText.size()) {
        const std::string item = extractArrayObject(arrayText, cursor);
        if (item.empty()) {
            break;
        }
        CallInfo call;
        std::string file;
        readStringField(item, "file", file);
        call.filePath = file.empty() ? fallbackFile : std::filesystem::path(file);
        readUintField(item, "line", call.line);
        readStringField(item, "name", call.name);
        readStringField(item, "target_qualified_name", call.targetQualifiedName);
        readStringField(item, "caller_qualified_name", call.callerQualifiedName);
        readStringField(item, "caller_stable_key", call.callerStableKey);
        if (!readStringField(item, "confidence", call.confidence)) {
            call.confidence = "heuristic";
        }
        calls.push_back(std::move(call));
    }
    return calls;
}

std::vector<SymbolRelationInfo> readRelations(
    const std::string& object,
    const std::filesystem::path& fallbackFile) {
    std::vector<SymbolRelationInfo> relations;
    const std::size_t arrayPos = object.find("\"relations\"");
    if (arrayPos == std::string::npos) {
        return relations;
    }
    std::size_t cursor = object.find('[', arrayPos);
    if (cursor == std::string::npos) {
        return relations;
    }
    const std::size_t arrayEnd = object.find(']', cursor);
    if (arrayEnd == std::string::npos) {
        return relations;
    }
    const std::string arrayText = object.substr(cursor, arrayEnd - cursor + 1);
    cursor = 0;
    while (cursor < arrayText.size()) {
        const std::string item = extractArrayObject(arrayText, cursor);
        if (item.empty()) {
            break;
        }
        SymbolRelationInfo relation;
        std::string file;
        readStringField(item, "file", file);
        relation.filePath = file.empty() ? fallbackFile : std::filesystem::path(file);
        readUintField(item, "line", relation.line);
        readStringField(item, "source_qualified_name", relation.sourceQualifiedName);
        readStringField(item, "source_stable_key", relation.sourceStableKey);
        readStringField(item, "target_name", relation.targetName);
        readStringField(item, "kind", relation.kind);
        if (!readStringField(item, "confidence", relation.confidence)) {
            relation.confidence = "heuristic";
        }
        relations.push_back(std::move(relation));
    }
    return relations;
}

std::vector<EntryPointInfo> readEntryPoints(
    const std::string& object,
    const std::filesystem::path& fallbackFile) {
    std::vector<EntryPointInfo> entries;
    const std::size_t arrayPos = object.find("\"entry_points\"");
    if (arrayPos == std::string::npos) {
        return entries;
    }
    std::size_t cursor = object.find('[', arrayPos);
    if (cursor == std::string::npos) {
        return entries;
    }
    const std::size_t arrayEnd = object.find(']', cursor);
    if (arrayEnd == std::string::npos) {
        return entries;
    }
    const std::string arrayText = object.substr(cursor, arrayEnd - cursor + 1);
    cursor = 0;
    while (cursor < arrayText.size()) {
        const std::string item = extractArrayObject(arrayText, cursor);
        if (item.empty()) {
            break;
        }
        EntryPointInfo entry;
        std::string file;
        readStringField(item, "file", file);
        entry.filePath = file.empty() ? fallbackFile : std::filesystem::path(file);
        readUintField(item, "line", entry.line);
        readStringField(item, "qualified_name", entry.qualifiedName);
        readStringField(item, "symbol_stable_key", entry.symbolStableKey);
        readStringField(item, "kind", entry.kind);
        if (!readStringField(item, "confidence", entry.confidence)) {
            entry.confidence = "heuristic";
        }
        entries.push_back(std::move(entry));
    }
    return entries;
}

} // namespace

std::unordered_map<std::string, FileInfo> ScanCache::loadPreviousFiles(
    const std::filesystem::path& previousScanJson,
    std::vector<std::string>& diagnostics) {
    std::unordered_map<std::string, FileInfo> cached;
    const std::string text = readAll(previousScanJson);
    if (text.empty()) {
        diagnostics.push_back("warning: failed to read previous scan JSON: " + previousScanJson.string());
        return cached;
    }

    const std::string filesMarker = "\"files\"";
    std::size_t filesPos = std::string::npos;
    std::size_t searchFrom = 0;
    while (searchFrom < text.size()) {
        const std::size_t candidate = text.find(filesMarker, searchFrom);
        if (candidate == std::string::npos) {
            break;
        }
        std::size_t lookAhead = candidate + filesMarker.size();
        while (lookAhead < text.size() &&
               std::isspace(static_cast<unsigned char>(text[lookAhead])) != 0) {
            ++lookAhead;
        }
        if (lookAhead < text.size() && text[lookAhead] == ':') {
            filesPos = candidate;
        }
        searchFrom = candidate + filesMarker.size();
    }

    if (filesPos == std::string::npos) {
        diagnostics.push_back("warning: previous scan JSON has no files array");
        return cached;
    }

    std::size_t cursor = text.find('[', filesPos);
    if (cursor == std::string::npos) {
        return cached;
    }

    while (cursor < text.size()) {
        const std::string object = extractArrayObject(text, cursor);
        if (object.empty()) {
            break;
        }

        FileInfo file;
        std::string path;
        if (!readStringField(object, "path", path)) {
            continue;
        }
        file.relativePath = path;
        readStringField(object, "extension", file.extension);
        readStringField(object, "category", file.category);
        std::uint64_t sizeBytes = 0;
        readUintField(object, "size_bytes", sizeBytes);
        file.sizeBytes = static_cast<std::uintmax_t>(sizeBytes);
        readUintField(object, "mtime_unix", file.mtimeUnix);
        readBoolField(object, "from_compile_commands", file.fromCompileCommands);
        file.lines = readLineMetrics(object);
        file.includes = readIncludes(object, file.relativePath);
        file.symbols = readSymbols(object, file.relativePath);
        file.calls = readCalls(object, file.relativePath);
        file.relations = readRelations(object, file.relativePath);
        file.entryPoints = readEntryPoints(object, file.relativePath);
        cached[file.relativePath.generic_string()] = std::move(file);
    }

    diagnostics.push_back(
        "info: loaded " + std::to_string(cached.size()) + " cached files from " + previousScanJson.string());
    return cached;
}

bool ScanCache::isReusable(
    const FileInfo& cached,
    std::uintmax_t sizeBytes,
    std::uint64_t mtimeUnix) {
    if (cached.sizeBytes != sizeBytes) {
        return false;
    }
    if (cached.mtimeUnix != 0 && mtimeUnix != 0) {
        return cached.mtimeUnix == mtimeUnix;
    }
    // Fall back to size-only reuse when filesystem mtimes are unavailable.
    return !cached.symbols.empty() || cached.lines.total > 0;
}

} // namespace cppinsight
