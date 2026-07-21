#include "project/ProjectScanner.h"

#include <algorithm>
#include <cctype>
#include <fstream>
#include <iterator>
#include <regex>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string_view>
#include <unordered_set>

namespace cppinsight {
namespace {

struct FileAnalysis {
    LineMetrics lines;
    std::vector<IncludeInfo> includes;
    std::vector<SymbolInfo> symbols;
    std::vector<CallInfo> calls;
    std::vector<std::string> diagnostics;
};

struct NamespaceScope {
    std::string name;
    int depth = 0;
};

bool isWhitespaceOnly(std::string_view line) {
    return std::all_of(line.begin(), line.end(), [](unsigned char character) {
        return std::isspace(character) != 0;
    });
}

std::string trim(std::string_view value) {
    std::size_t begin = 0;
    while (begin < value.size() && std::isspace(static_cast<unsigned char>(value[begin])) != 0) {
        ++begin;
    }

    std::size_t end = value.size();
    while (end > begin && std::isspace(static_cast<unsigned char>(value[end - 1])) != 0) {
        --end;
    }

    return std::string(value.substr(begin, end - begin));
}

bool contains(const std::set<std::string>& values, const std::string& value) {
    return values.find(value) != values.end();
}

void classifyLine(std::string_view line, bool& insideBlockComment, LineMetrics& metrics) {
    ++metrics.total;

    if (!insideBlockComment && isWhitespaceOnly(line)) {
        ++metrics.blank;
        return;
    }

    bool hasCode = false;
    bool hasComment = false;

    for (std::size_t index = 0; index < line.size();) {
        const char current = line[index];
        const char next = index + 1 < line.size() ? line[index + 1] : '\0';

        if (insideBlockComment) {
            hasComment = true;
            if (current == '*' && next == '/') {
                insideBlockComment = false;
                index += 2;
            } else {
                ++index;
            }
            continue;
        }

        if (std::isspace(static_cast<unsigned char>(current)) != 0) {
            ++index;
            continue;
        }

        if (current == '/' && next == '/') {
            hasComment = true;
            break;
        }

        if (current == '/' && next == '*') {
            hasComment = true;
            insideBlockComment = true;
            index += 2;
            continue;
        }

        hasCode = true;
        ++index;
    }

    if (hasCode) {
        ++metrics.code;
    } else if (hasComment) {
        ++metrics.comment;
    } else {
        ++metrics.blank;
    }
}

std::string stripCommentsPreserveStrings(std::string_view line, bool& insideBlockComment) {
    std::string output;
    output.reserve(line.size());

    bool insideString = false;
    bool escaped = false;
    char delimiter = '\0';

    for (std::size_t index = 0; index < line.size();) {
        const char current = line[index];
        const char next = index + 1 < line.size() ? line[index + 1] : '\0';

        if (insideBlockComment) {
            if (current == '*' && next == '/') {
                insideBlockComment = false;
                output.append("  ");
                index += 2;
            } else {
                output.push_back(' ');
                ++index;
            }
            continue;
        }

        if (insideString) {
            output.push_back(current);
            if (escaped) {
                escaped = false;
            } else if (current == '\\') {
                escaped = true;
            } else if (current == delimiter) {
                insideString = false;
                delimiter = '\0';
            }
            ++index;
            continue;
        }

        if (current == '"' || current == '\'') {
            insideString = true;
            delimiter = current;
            output.push_back(current);
            ++index;
            continue;
        }

        if (current == '/' && next == '/') {
            output.append(line.size() - index, ' ');
            break;
        }

        if (current == '/' && next == '*') {
            insideBlockComment = true;
            output.append("  ");
            index += 2;
            continue;
        }

        output.push_back(current);
        ++index;
    }

    return output;
}

std::string blankStringLiteralContents(std::string value) {
    bool insideString = false;
    bool escaped = false;
    char delimiter = '\0';

    for (char& character : value) {
        if (insideString) {
            if (escaped) {
                escaped = false;
                character = ' ';
                continue;
            }

            if (character == '\\') {
                escaped = true;
                character = ' ';
                continue;
            }

            if (character == delimiter) {
                insideString = false;
                delimiter = '\0';
            } else {
                character = ' ';
            }
            continue;
        }

        if (character == '"' || character == '\'') {
            insideString = true;
            delimiter = character;
        }
    }

    return value;
}

std::string topLevelName(const std::filesystem::path& relativePath) {
    auto iterator = relativePath.begin();
    if (iterator == relativePath.end()) {
        return ".";
    }

    return iterator->string();
}

void addLineMetrics(LineMetrics& target, const LineMetrics& source) {
    target.total += source.total;
    target.blank += source.blank;
    target.comment += source.comment;
    target.code += source.code;
}

std::string currentScope(const std::vector<NamespaceScope>& namespaceScopes) {
    std::ostringstream output;
    for (std::size_t index = 0; index < namespaceScopes.size(); ++index) {
        if (index != 0) {
            output << "::";
        }
        output << namespaceScopes[index].name;
    }
    return output.str();
}

int countCharacter(const std::string& value, char expected) {
    return static_cast<int>(std::count(value.begin(), value.end(), expected));
}

std::string extractFunctionName(const std::string& declarator) {
    static const std::regex nameRegex(R"(([A-Za-z_~][A-Za-z0-9_:~]*)\s*$)");
    std::smatch match;
    if (std::regex_search(declarator, match, nameRegex)) {
        const std::string name = match[1].str();
        const std::size_t separator = name.rfind("::");
        if (separator != std::string::npos) {
            return name.substr(separator + 2);
        }
        return name;
    }

    return {};
}

bool looksLikeFunctionLine(const std::string& code) {
    const std::string trimmed = trim(code);
    if (trimmed.empty() || trimmed[0] == '#') {
        return false;
    }

    static const std::set<std::string> disallowedPrefixes = {
        "if", "for", "while", "switch", "catch", "return", "sizeof", "static_assert"
    };

    for (const std::string& prefix : disallowedPrefixes) {
        if (trimmed == prefix || trimmed.rfind(prefix + " ", 0) == 0 || trimmed.rfind(prefix + "(", 0) == 0) {
            return false;
        }
    }

    return trimmed.find('(') != std::string::npos && trimmed.find(')') != std::string::npos;
}

void extractInclude(
    const std::string& code,
    const std::filesystem::path& relativePath,
    std::uint64_t lineNumber,
    std::vector<IncludeInfo>& includes) {
    static const std::regex includeRegex(R"(^\s*#\s*include\s*([<"])([^>"]+)[>"])");
    std::smatch match;
    if (!std::regex_search(code, match, includeRegex)) {
        return;
    }

    IncludeInfo include;
    include.filePath = relativePath;
    include.line = lineNumber;
    include.target = match[2].str();
    include.kind = match[1].str() == "<" ? "system" : "local";
    includes.push_back(std::move(include));
}

bool extractTypeSymbol(
    const std::string& code,
    const std::filesystem::path& relativePath,
    std::uint64_t lineNumber,
    const std::string& scope,
    std::vector<SymbolInfo>& symbols) {
    static const std::regex typeRegex(
        R"(^\s*(?:template\s*<[^>]+>\s*)?(class|struct|enum(?:\s+(?:class|struct))?)\s+([A-Za-z_][A-Za-z0-9_]*)\b)");

    std::smatch match;
    if (!std::regex_search(code, match, typeRegex)) {
        return false;
    }

    SymbolInfo symbol;
    symbol.filePath = relativePath;
    symbol.line = lineNumber;
    symbol.kind = match[1].str();
    if (symbol.kind.rfind("enum", 0) == 0) {
        symbol.kind = "enum";
    }
    symbol.name = match[2].str();
    symbol.signature = trim(code);
    symbol.scope = scope;
    symbol.isDefinition = code.find('{') != std::string::npos;
    symbols.push_back(std::move(symbol));
    return true;
}

std::string extractFunctionSymbol(
    const std::string& code,
    const std::filesystem::path& relativePath,
    std::uint64_t lineNumber,
    const std::string& scope,
    std::vector<SymbolInfo>& symbols) {
    if (!looksLikeFunctionLine(code)) {
        return {};
    }

    static const std::regex functionRegex(
        R"(^\s*(?:template\s*<[^>]*>\s*)?((?:[A-Za-z_~][A-Za-z0-9_:<>~,&*\s]+\s+)?[A-Za-z_~][A-Za-z0-9_:~]*)\s*\([^;{}()]*\)\s*(?:const\s*)?(?:noexcept\s*)?(?:->\s*[A-Za-z_][A-Za-z0-9_:<>,\s*&]*)?\s*([;{])?)");

    std::smatch match;
    if (!std::regex_search(code, match, functionRegex)) {
        return {};
    }

    const std::string name = extractFunctionName(match[1].str());
    if (name.empty()) {
        return {};
    }

    static const std::set<std::string> keywords = {
        "if", "for", "while", "switch", "catch", "return", "sizeof", "alignof", "decltype"
    };
    if (contains(keywords, name)) {
        return {};
    }

    const std::string trimmedLine = trim(code);
    const bool hasBody = trimmedLine.find('{') != std::string::npos;
    const bool hasDeclarationTerminator = trimmedLine.find(';') != std::string::npos;
    if (!hasBody && !hasDeclarationTerminator) {
        return {};
    }

    SymbolInfo symbol;
    symbol.filePath = relativePath;
    symbol.line = lineNumber;
    symbol.kind = "function";
    symbol.name = name;
    symbol.signature = trimmedLine;
    symbol.scope = scope;
    symbol.isDefinition = hasBody;
    symbols.push_back(std::move(symbol));
    return name;
}

void extractCalls(
    const std::string& code,
    const std::filesystem::path& relativePath,
    std::uint64_t lineNumber,
    const std::string& functionSymbolOnLine,
    std::vector<CallInfo>& calls) {
    static const std::regex callRegex(R"(\b([A-Za-z_][A-Za-z0-9_]*)\s*\()");
    static const std::set<std::string> ignoredNames = {
        "if", "for", "while", "switch", "catch", "return", "sizeof", "alignof", "decltype",
        "static_cast", "reinterpret_cast", "const_cast", "dynamic_cast"
    };

    std::set<std::string> emittedOnLine;
    for (std::sregex_iterator iterator(code.begin(), code.end(), callRegex), end; iterator != end; ++iterator) {
        const std::string name = (*iterator)[1].str();
        if (name == functionSymbolOnLine || contains(ignoredNames, name)) {
            continue;
        }

        if (!emittedOnLine.insert(name).second) {
            continue;
        }

        CallInfo call;
        call.filePath = relativePath;
        call.line = lineNumber;
        call.name = name;
        calls.push_back(std::move(call));
    }
}

FileAnalysis analyzeFile(const std::filesystem::path& absolutePath, const std::filesystem::path& relativePath) {
    std::ifstream input(absolutePath, std::ios::binary);
    if (!input) {
        FileAnalysis failed;
        failed.diagnostics.push_back("warning: failed to open file: " + relativePath.generic_string());
        return failed;
    }

    FileAnalysis analysis;
    bool insideBlockCommentForMetrics = false;
    bool insideBlockCommentForAnalysis = false;
    int braceDepth = 0;
    std::uint64_t lineNumber = 0;
    std::vector<NamespaceScope> namespaceScopes;

    std::string buffer;
    while (std::getline(input, buffer)) {
        ++lineNumber;
        classifyLine(buffer, insideBlockCommentForMetrics, analysis.lines);

        const std::string codeWithStrings =
            stripCommentsPreserveStrings(buffer, insideBlockCommentForAnalysis);
        const std::string code = blankStringLiteralContents(codeWithStrings);
        const std::string scope = currentScope(namespaceScopes);

        extractInclude(codeWithStrings, relativePath, lineNumber, analysis.includes);
        const bool isTypeSymbol =
            extractTypeSymbol(code, relativePath, lineNumber, scope, analysis.symbols);
        const std::string functionSymbolOnLine =
            isTypeSymbol ? std::string() : extractFunctionSymbol(code, relativePath, lineNumber, scope, analysis.symbols);
        extractCalls(code, relativePath, lineNumber, functionSymbolOnLine, analysis.calls);

        static const std::regex namespaceRegex(R"(^\s*namespace\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{)");
        std::smatch namespaceMatch;
        const bool opensNamespace = std::regex_search(code, namespaceMatch, namespaceRegex);
        const NamespaceScope pendingNamespace = opensNamespace
            ? NamespaceScope{namespaceMatch[1].str(), braceDepth + 1}
            : NamespaceScope{};

        if (opensNamespace) {
            namespaceScopes.push_back(pendingNamespace);
        }

        braceDepth += countCharacter(code, '{');
        braceDepth -= countCharacter(code, '}');
        if (braceDepth < 0) {
            braceDepth = 0;
        }

        while (!namespaceScopes.empty() && braceDepth < namespaceScopes.back().depth) {
            namespaceScopes.pop_back();
        }
    }

    return analysis;
}

template <typename T>
void appendAll(std::vector<T>& target, const std::vector<T>& source) {
    target.insert(target.end(), source.begin(), source.end());
}

} // namespace

ScanResult ProjectScanner::scan(const std::filesystem::path& rootPath) const {
    ScanResult result;
    result.rootPath = std::filesystem::absolute(rootPath);

    if (!std::filesystem::exists(result.rootPath)) {
        throw std::runtime_error("Project path does not exist: " + result.rootPath.string());
    }

    if (!std::filesystem::is_directory(result.rootPath)) {
        throw std::runtime_error("Project path is not a directory: " + result.rootPath.string());
    }

    std::filesystem::recursive_directory_iterator iterator(
        result.rootPath,
        std::filesystem::directory_options::skip_permission_denied);
    const std::filesystem::recursive_directory_iterator end;

    for (; iterator != end; ++iterator) {
        const auto& entry = *iterator;

        if (entry.is_directory() && shouldSkipDirectory(entry.path())) {
            iterator.disable_recursion_pending();
            continue;
        }

        if (!entry.is_regular_file()) {
            continue;
        }

        if (!isCppFile(entry.path())) {
            continue;
        }

        FileInfo file;
        file.absolutePath = std::filesystem::absolute(entry.path());
        file.relativePath = std::filesystem::relative(file.absolutePath, result.rootPath);
        file.extension = normalizeExtension(file.absolutePath.extension().string());
        file.category = categorizeExtension(file.extension);

        std::error_code fileSizeError;
        file.sizeBytes = entry.file_size(fileSizeError);
        if (fileSizeError) {
            file.sizeBytes = 0;
            ++result.skippedEntries;
            result.diagnostics.push_back("warning: failed to read file size: " + file.relativePath.generic_string());
        }

        const FileAnalysis analysis = analyzeFile(file.absolutePath, file.relativePath);
        file.lines = analysis.lines;
        file.includes = analysis.includes;
        file.symbols = analysis.symbols;
        file.calls = analysis.calls;

        appendAll(result.diagnostics, analysis.diagnostics);
        appendAll(result.includes, file.includes);
        appendAll(result.symbols, file.symbols);
        appendAll(result.calls, file.calls);

        result.stats.totalBytes += file.sizeBytes;
        addLineMetrics(result.stats.lines, file.lines);
        ++result.extensionCounts[file.extension];
        ++result.topLevelDirectoryCounts[topLevelName(file.relativePath)];

        if (file.category == "source") {
            ++result.stats.sourceFiles;
        } else if (file.category == "header") {
            ++result.stats.headerFiles;
        } else if (file.category == "inline") {
            ++result.stats.inlineFiles;
        }

        result.files.push_back(std::move(file));
    }

    std::sort(result.files.begin(), result.files.end(), [](const FileInfo& left, const FileInfo& right) {
        return left.relativePath.string() < right.relativePath.string();
    });

    return result;
}

bool ProjectScanner::isCppFile(const std::filesystem::path& path) {
    static const std::unordered_set<std::string> extensions = {
        ".c", ".cc", ".cpp", ".cxx",
        ".h", ".hh", ".hpp", ".hxx",
        ".inl", ".ipp"
    };

    return extensions.find(normalizeExtension(path.extension().string())) != extensions.end();
}

bool ProjectScanner::shouldSkipDirectory(const std::filesystem::path& path) {
    static const std::unordered_set<std::string> names = {
        ".git", ".svn", ".hg",
        "build", "build-codex", "cmake-build-debug", "cmake-build-release",
        "node_modules", ".cache", ".vs", ".idea", ".vscode", "target", "dist"
    };

    return names.find(path.filename().string()) != names.end();
}

std::string ProjectScanner::categorizeExtension(const std::string& extension) {
    static const std::unordered_set<std::string> sourceExtensions = {
        ".c", ".cc", ".cpp", ".cxx"
    };

    static const std::unordered_set<std::string> headerExtensions = {
        ".h", ".hh", ".hpp", ".hxx"
    };

    static const std::unordered_set<std::string> inlineExtensions = {
        ".inl", ".ipp"
    };

    if (sourceExtensions.find(extension) != sourceExtensions.end()) {
        return "source";
    }

    if (headerExtensions.find(extension) != headerExtensions.end()) {
        return "header";
    }

    if (inlineExtensions.find(extension) != inlineExtensions.end()) {
        return "inline";
    }

    return "other";
}

std::string ProjectScanner::normalizeExtension(std::string extension) {
    std::transform(extension.begin(), extension.end(), extension.begin(), [](unsigned char character) {
        return static_cast<char>(std::tolower(character));
    });
    return extension;
}

} // namespace cppinsight
