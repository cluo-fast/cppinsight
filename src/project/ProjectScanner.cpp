#include "project/ProjectScanner.h"

#include "project/CompileCommands.h"
#include "project/ScanCache.h"

#include <algorithm>
#include <cctype>
#include <chrono>
#include <fstream>
#include <iomanip>
#include <iterator>
#include <optional>
#include <regex>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string_view>
#include <unordered_map>
#include <unordered_set>

namespace cppinsight {
namespace {

struct FileAnalysis {
    LineMetrics lines;
    std::vector<IncludeInfo> includes;
    std::vector<SymbolInfo> symbols;
    std::vector<CallInfo> calls;
    std::vector<SymbolRelationInfo> relations;
    std::vector<EntryPointInfo> entryPoints;
    std::vector<std::string> diagnostics;
    std::unordered_set<std::string> macroNames;
};

struct NamespaceScope {
    std::string name;
    int depth = 0;
};

struct TypeScope {
    std::string name;
    std::string qualifiedName;
    std::string stableKey;
    int depth = 0;
};

struct FunctionScope {
    std::string qualifiedName;
    std::string stableKey;
    int depth = 0;
};

struct ExtractedFunction {
    std::string name;
    std::string qualifiedName;
    std::string stableKey;
    bool hasBody = false;
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

std::string secondLevelName(const std::filesystem::path& relativePath) {
    auto iterator = relativePath.begin();
    if (iterator == relativePath.end()) {
        return ".";
    }

    const std::string first = iterator->string();
    ++iterator;
    if (iterator == relativePath.end() || iterator->string().empty()) {
        return first;
    }

    return first + "/" + iterator->string();
}

std::uint64_t fileMtimeUnix(const std::filesystem::path& path) {
    std::error_code error;
    const auto writeTime = std::filesystem::last_write_time(path, error);
    if (error) {
        return 0;
    }

    // Use the filesystem clock epoch for stable equality checks across scans.
    const auto ticks = writeTime.time_since_epoch();
    const auto seconds = std::chrono::duration_cast<std::chrono::seconds>(ticks).count();
    if (seconds > 0) {
        return static_cast<std::uint64_t>(seconds);
    }

    return static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::milliseconds>(ticks).count());
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

std::string joinQualified(const std::string& scope, const std::string& name) {
    if (scope.empty()) {
        return name;
    }
    if (name.empty()) {
        return scope;
    }
    return scope + "::" + name;
}

std::string currentQualifiedScope(
    const std::vector<NamespaceScope>& namespaceScopes,
    const std::vector<TypeScope>& typeScopes) {
    std::string scope = currentScope(namespaceScopes);
    for (const TypeScope& type : typeScopes) {
        scope = joinQualified(scope, type.name);
    }
    return scope;
}

std::string parentQualifiedName(const std::string& qualifiedName) {
    const std::size_t separator = qualifiedName.rfind("::");
    return separator == std::string::npos ? std::string() : qualifiedName.substr(0, separator);
}

std::string normalizeSignature(std::string_view signature) {
    std::string normalized;
    normalized.reserve(signature.size());
    bool pendingSpace = false;
    for (unsigned char character : signature) {
        if (std::isspace(character) != 0) {
            pendingSpace = !normalized.empty();
            continue;
        }
        if (pendingSpace) {
            normalized.push_back(' ');
            pendingSpace = false;
        }
        normalized.push_back(static_cast<char>(character));
    }
    return normalized;
}

std::string normalizedPath(const std::filesystem::path& path) {
    return path.lexically_normal().generic_string();
}

std::string stableSymbolKey(
    const std::filesystem::path& file,
    const std::string& kind,
    const std::string& qualifiedName,
    const std::string& signature) {
    const std::string canonical =
        kind + "\n" + normalizedPath(file) + "\n" + qualifiedName + "\n" +
        normalizeSignature(signature);
    std::uint64_t hash = 14695981039346656037ULL;
    for (unsigned char character : canonical) {
        hash ^= static_cast<std::uint64_t>(character);
        hash *= 1099511628211ULL;
    }
    std::ostringstream output;
    output << "sym:" << std::hex << std::nouppercase << std::setw(16)
           << std::setfill('0') << hash;
    return output.str();
}

std::string stableKeyForQualifiedName(
    const std::vector<SymbolInfo>& symbols,
    const std::string& qualifiedName) {
    for (auto iterator = symbols.rbegin(); iterator != symbols.rend(); ++iterator) {
        if (iterator->qualifiedName == qualifiedName && iterator->isDefinition) {
            return iterator->stableKey;
        }
    }
    for (auto iterator = symbols.rbegin(); iterator != symbols.rend(); ++iterator) {
        if (iterator->qualifiedName == qualifiedName) {
            return iterator->stableKey;
        }
    }
    return {};
}

std::string qualifyTypeName(const std::string& rawName, const std::string& sourceType) {
    std::string target = trim(rawName);
    while (target.rfind("::", 0) == 0) {
        target.erase(0, 2);
    }
    const std::size_t templateOpen = target.find('<');
    if (templateOpen != std::string::npos) {
        target = trim(target.substr(0, templateOpen));
    }
    if (target.empty() || target.find("::") != std::string::npos) {
        return target;
    }
    return joinQualified(parentQualifiedName(sourceType), target);
}

int countCharacter(const std::string& value, char expected) {
    return static_cast<int>(std::count(value.begin(), value.end(), expected));
}

bool isIdentifierStart(unsigned char character) {
    return character == '_' || character == '~' || std::isalpha(character) != 0;
}

bool isIdentifierPart(unsigned char character) {
    return character == '_' || character == '~' || std::isalnum(character) != 0;
}

// ALL_CAPS names are treated as macro invocations, not calls or definitions.
// A real function with that spelling is rare; keeping it would leave the
// FMT_* / CHECK style false positives the heuristic cannot otherwise see.
bool isAllCapsMacroName(const std::string& name) {
    if (name.size() < 2) {
        return false;
    }
    bool hasUpper = false;
    for (unsigned char character : name) {
        if (std::islower(character) != 0) {
            return false;
        }
        if (std::isupper(character) != 0) {
            hasUpper = true;
            continue;
        }
        if (character != '_' && std::isdigit(character) == 0) {
            return false;
        }
    }
    return hasUpper;
}

bool isControlFlowStart(const std::string& name) {
    return name == "if" || name == "for" || name == "while" || name == "switch" ||
        name == "catch" || name == "return";
}

bool isNonCallableName(const std::string& name) {
    static const std::unordered_set<std::string> names = {
        "if", "for", "while", "switch", "catch", "return",
        "sizeof", "alignof", "decltype", "static_assert", "noexcept", "alignas",
        "requires", "co_await", "co_yield", "co_return", "typeid", "defined",
        "static_cast", "reinterpret_cast", "const_cast", "dynamic_cast",
        "__has_include", "__has_feature", "__has_cpp_attribute",
        "__has_builtin", "__has_extension",
        "bool", "char", "int", "void", "float", "double", "auto",
        "long", "short", "signed", "unsigned", "wchar_t",
        "char8_t", "char16_t", "char32_t"
    };
    return names.find(name) != names.end();
}

bool isPreprocessorLine(const std::string& code) {
    const std::string trimmed = trim(code);
    return !trimmed.empty() && trimmed[0] == '#';
}

bool lineContinues(const std::string& code) {
    const std::string trimmed = trim(code);
    return !trimmed.empty() && trimmed.back() == '\\';
}

std::string firstIdentifier(const std::string& text) {
    std::size_t cursor = 0;
    while (cursor < text.size() && std::isspace(static_cast<unsigned char>(text[cursor])) != 0) {
        ++cursor;
    }
    const std::size_t start = cursor;
    if (start >= text.size() || !isIdentifierStart(static_cast<unsigned char>(text[start]))) {
        return {};
    }
    ++cursor;
    while (cursor < text.size() && isIdentifierPart(static_cast<unsigned char>(text[cursor]))) {
        ++cursor;
    }
    return text.substr(start, cursor - start);
}

void collectMacroDefine(const std::string& code, std::unordered_set<std::string>& names) {
    const std::string trimmed = trim(code);
    if (trimmed.empty() || trimmed[0] != '#') {
        return;
    }
    std::size_t cursor = 1;
    while (cursor < trimmed.size() && std::isspace(static_cast<unsigned char>(trimmed[cursor])) != 0) {
        ++cursor;
    }
    constexpr std::string_view keyword = "define";
    if (trimmed.compare(cursor, keyword.size(), keyword.data()) != 0) {
        return;
    }
    cursor += keyword.size();
    if (cursor >= trimmed.size() || std::isspace(static_cast<unsigned char>(trimmed[cursor])) == 0) {
        return;
    }
    while (cursor < trimmed.size() && std::isspace(static_cast<unsigned char>(trimmed[cursor])) != 0) {
        ++cursor;
    }
    const std::size_t start = cursor;
    if (start >= trimmed.size() || !isIdentifierStart(static_cast<unsigned char>(trimmed[start]))) {
        return;
    }
    ++cursor;
    while (cursor < trimmed.size() && isIdentifierPart(static_cast<unsigned char>(trimmed[cursor]))) {
        ++cursor;
    }
    names.insert(trimmed.substr(start, cursor - start));
}

// `.size(`, `obj->foo(`, and `: member_(` are not definitions.
bool nameIsPrecededByMemberOrInitializer(const std::string& text, std::size_t openParen) {
    std::size_t cursor = openParen;
    while (cursor > 0 && std::isspace(static_cast<unsigned char>(text[cursor - 1])) != 0) {
        --cursor;
    }
    while (cursor > 0 && isIdentifierPart(static_cast<unsigned char>(text[cursor - 1]))) {
        --cursor;
    }
    while (cursor > 0 && std::isspace(static_cast<unsigned char>(text[cursor - 1])) != 0) {
        --cursor;
    }
    if (cursor == 0) {
        return false;
    }
    const char previous = text[cursor - 1];
    if (previous == '.' || previous == '>') {
        return true;
    }
    return previous == ':' && (cursor < 2 || text[cursor - 2] != ':');
}

std::string extractFunctionName(const std::string& declarator) {
    // Avoid std::regex here: MSVC throws regex_error(error_complexity) on long/noisy lines.
    const std::string trimmed = trim(declarator);
    if (trimmed.empty()) {
        return {};
    }

    std::size_t end = trimmed.size();
    while (end > 0 && std::isspace(static_cast<unsigned char>(trimmed[end - 1])) != 0) {
        --end;
    }
    if (end == 0) {
        return {};
    }

    std::size_t start = end;
    while (start > 0 && isIdentifierPart(static_cast<unsigned char>(trimmed[start - 1]))) {
        --start;
    }
    if (start >= end || !isIdentifierStart(static_cast<unsigned char>(trimmed[start]))) {
        return {};
    }

    const std::string name = trimmed.substr(start, end - start);
    return name == "~" ? std::string() : name;
}

std::size_t findMatchingParen(const std::string& text, std::size_t openIndex) {
    if (openIndex >= text.size() || text[openIndex] != '(') {
        return std::string::npos;
    }

    int depth = 0;
    for (std::size_t index = openIndex; index < text.size(); ++index) {
        const char character = text[index];
        if (character == '(') {
            ++depth;
        } else if (character == ')') {
            --depth;
            if (depth == 0) {
                return index;
            }
        }
    }
    return std::string::npos;
}

std::string stripLeadingTemplate(const std::string& declarator) {
    const std::string trimmed = trim(declarator);
    if (trimmed.rfind("template", 0) != 0) {
        return trimmed;
    }

    const std::size_t open = trimmed.find('<');
    if (open == std::string::npos) {
        return trimmed;
    }

    int depth = 0;
    for (std::size_t index = open; index < trimmed.size(); ++index) {
        if (trimmed[index] == '<') {
            ++depth;
        } else if (trimmed[index] == '>') {
            --depth;
            if (depth == 0) {
                return trim(trimmed.substr(index + 1));
            }
        }
    }
    return trimmed;
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
    try {
        if (!std::regex_search(code, match, includeRegex)) {
            return;
        }
    } catch (const std::regex_error&) {
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
        R"(^\s*(?:template\s*<[^;{]{0,256}>\s*)?(class|struct|enum(?:\s+(?:class|struct))?)\s+([A-Za-z_][A-Za-z0-9_]*)\b)");

    std::smatch match;
    try {
        if (!std::regex_search(code, match, typeRegex)) {
            return false;
        }
    } catch (const std::regex_error&) {
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
    symbol.qualifiedName = joinQualified(scope, symbol.name);
    symbol.signature = trim(code);
    symbol.scope = scope;
    symbol.isDefinition = code.find('{') != std::string::npos;
    symbol.confidence = "heuristic";
    symbol.parentQualifiedName = scope;
    symbol.parentStableKey = stableKeyForQualifiedName(symbols, scope);
    symbol.stableKey = stableSymbolKey(
        relativePath, symbol.kind, symbol.qualifiedName, symbol.signature);
    symbols.push_back(std::move(symbol));
    return true;
}

std::string extractCallablePath(const std::string& declarator) {
    const std::string trimmed = trim(declarator);
    std::size_t end = trimmed.size();
    while (end > 0 && std::isspace(static_cast<unsigned char>(trimmed[end - 1])) != 0) {
        --end;
    }
    std::size_t start = end;
    while (start > 0) {
        const unsigned char character = static_cast<unsigned char>(trimmed[start - 1]);
        if (!isIdentifierPart(character) && character != ':') {
            break;
        }
        --start;
    }
    return trim(trimmed.substr(start, end - start));
}

ExtractedFunction extractFunctionSymbol(
    const std::string& code,
    const std::filesystem::path& relativePath,
    std::uint64_t lineNumber,
    const std::string& scope,
    bool allowDeclaration,
    std::vector<SymbolInfo>& symbols) {
    if (!looksLikeFunctionLine(code)) {
        return ExtractedFunction{};
    }

    const std::string trimmedLine = trim(code);
    const std::size_t openParen = trimmedLine.find('(');
    if (openParen == std::string::npos || openParen == 0) {
        return ExtractedFunction{};
    }
    if (findMatchingParen(trimmedLine, openParen) == std::string::npos) {
        return ExtractedFunction{};
    }

    const std::string declarator = stripLeadingTemplate(trimmedLine.substr(0, openParen));
    const std::string name = extractFunctionName(declarator);
    if (name.empty() || isNonCallableName(name) || isAllCapsMacroName(name)) {
        return ExtractedFunction{};
    }
    if (nameIsPrecededByMemberOrInitializer(trimmedLine, openParen)) {
        return ExtractedFunction{};
    }

    const bool hasBody = trimmedLine.find('{') != std::string::npos;
    const bool hasDeclarationTerminator = trimmedLine.find(';') != std::string::npos;
    if (!hasBody && !hasDeclarationTerminator) {
        return ExtractedFunction{};
    }
    // A named function cannot be declared inside a C++ function body. Without
    // this context guard, constructor expressions such as
    // `Engine engine("demo");` and member calls such as `engine.run();` can be
    // mistaken for function declarations and steal call ownership from the
    // enclosing entry point.
    if (!hasBody && !allowDeclaration) {
        return ExtractedFunction{};
    }

    // Bare `foo();` calls look like declarations; require a return type or destructor.
    const bool hasTypeOrScope =
        declarator.find(' ') != std::string::npos ||
        (!declarator.empty() && declarator[0] == '~');
    if (!hasTypeOrScope && !hasBody) {
        return ExtractedFunction{};
    }

    SymbolInfo symbol;
    symbol.filePath = relativePath;
    symbol.line = lineNumber;
    symbol.kind = "function";
    symbol.name = name;
    const std::string callablePath = extractCallablePath(declarator);
    symbol.qualifiedName = callablePath.find("::") == std::string::npos
        ? joinQualified(scope, name)
        : joinQualified(scope, callablePath);
    symbol.signature = trimmedLine;
    symbol.scope = scope;
    symbol.isDefinition = hasBody;
    symbol.confidence = "heuristic";
    symbol.parentQualifiedName = parentQualifiedName(symbol.qualifiedName);
    symbol.parentStableKey = stableKeyForQualifiedName(symbols, symbol.parentQualifiedName);
    symbol.stableKey = stableSymbolKey(
        relativePath, symbol.kind, symbol.qualifiedName, symbol.signature);
    ExtractedFunction extracted{
        symbol.name,
        symbol.qualifiedName,
        symbol.stableKey,
        hasBody,
    };
    symbols.push_back(std::move(symbol));
    return extracted;
}

void extractCalls(
    const std::string& code,
    const std::filesystem::path& relativePath,
    std::uint64_t lineNumber,
    const std::string& functionSymbolOnLine,
    const std::string& callerQualifiedName,
    const std::string& callerStableKey,
    std::vector<CallInfo>& calls) {
    // Calls outside a function body cannot be attributed safely. In
    // particular, declarations in headers and namespace-scope prototypes
    // otherwise look like call expressions to the lexical scanner. Keep an
    // inline function-body line as a fallback for pathological signatures;
    // the index resolver will only persist it when it can recover an owner.
    const std::string trimmedForDirective = trim(code);
    if (!trimmedForDirective.empty() && trimmedForDirective[0] == '#') {
        return;
    }
    if (!trimmedForDirective.empty() && trimmedForDirective[0] == ':') {
        // Constructor initializer lists (`: member_(args), other_(args) {`).
        return;
    }
    const bool hasInlineBody = code.find('{') != std::string::npos;
    if (callerQualifiedName.empty() && !hasInlineBody) {
        return;
    }
    static const std::set<std::string> expressionPrefixNames = {
        "case", "co_await", "co_return", "delete", "new", "return", "throw"
    };

    // Manual scan: std::regex on huge office macros hits error_complexity under MSVC.
    std::set<std::string> emittedOnLine;
    for (std::size_t index = 0; index < code.size(); ++index) {
        const unsigned char current = static_cast<unsigned char>(code[index]);
        if (!isIdentifierStart(current) || current == '~') {
            continue;
        }
        if (index > 0) {
            const unsigned char previous = static_cast<unsigned char>(code[index - 1]);
            if (isIdentifierPart(previous)) {
                continue;
            }
        }

        std::size_t end = index + 1;
        while (end < code.size() && isIdentifierPart(static_cast<unsigned char>(code[end]))) {
            ++end;
        }
        // Keep the complete qualified callee (for example, ns::helper()) so
        // resolver ranking can use the same qualified-name key as definitions.
        std::size_t qualifiedEnd = end;
        while (qualifiedEnd < code.size()) {
            std::size_t scopeCursor = qualifiedEnd;
            while (scopeCursor < code.size() && std::isspace(static_cast<unsigned char>(code[scopeCursor])) != 0) {
                ++scopeCursor;
            }
            if (scopeCursor + 1 >= code.size() || code[scopeCursor] != ':' || code[scopeCursor + 1] != ':') {
                break;
            }
            scopeCursor += 2;
            while (scopeCursor < code.size() && std::isspace(static_cast<unsigned char>(code[scopeCursor])) != 0) {
                ++scopeCursor;
            }
            if (scopeCursor >= code.size() || !isIdentifierStart(static_cast<unsigned char>(code[scopeCursor]))) {
                break;
            }
            std::size_t componentEnd = scopeCursor + 1;
            while (componentEnd < code.size() && isIdentifierPart(static_cast<unsigned char>(code[componentEnd]))) {
                ++componentEnd;
            }
            qualifiedEnd = componentEnd;
        }
        std::size_t cursor = qualifiedEnd;
        while (cursor < code.size() && std::isspace(static_cast<unsigned char>(code[cursor])) != 0) {
            ++cursor;
        }
        if (cursor >= code.size() || code[cursor] != '(') {
            continue;
        }

        const std::size_t nameStart = index;
        const std::string name = code.substr(nameStart, qualifiedEnd - nameStart);
        const std::size_t lastScope = name.rfind("::");
        const std::string leafName = lastScope == std::string::npos ? name : name.substr(lastScope + 2);
        index = qualifiedEnd - 1;
        if (leafName == functionSymbolOnLine || isNonCallableName(leafName) ||
            isAllCapsMacroName(leafName)) {
            continue;
        }
        if (lastScope == std::string::npos) {
            std::size_t previous = nameStart;
            while (previous > 0 && std::isspace(static_cast<unsigned char>(code[previous - 1])) != 0) {
                --previous;
            }
            if (previous > 0 && code[previous - 1] == ':' &&
                (previous < 2 || code[previous - 2] != ':')) {
                // Constructor member initializers (`: member_(value)`) are
                // not calls to the member field.
                continue;
            }
            if (previous > 0 && isIdentifierPart(static_cast<unsigned char>(code[previous - 1]))) {
                std::size_t prefixStart = previous - 1;
                while (prefixStart > 0 &&
                       isIdentifierPart(static_cast<unsigned char>(code[prefixStart - 1]))) {
                    --prefixStart;
                }
                const std::string prefix = code.substr(prefixStart, previous - prefixStart);
                if (!contains(expressionPrefixNames, prefix)) {
                    // `Type value(args)` is a local construction, not a call
                    // to `value`; qualified/member calls are handled above.
                    continue;
                }
            }
        }
        if (!emittedOnLine.insert(name).second) {
            continue;
        }

        CallInfo call;
        call.filePath = relativePath;
        call.line = lineNumber;
        call.name = name;
        call.targetQualifiedName = name;
        call.callerQualifiedName = callerQualifiedName;
        call.callerStableKey = callerStableKey;
        call.confidence = "heuristic";
        calls.push_back(std::move(call));
    }
}

void extractBaseRelations(
    const std::string& code,
    const SymbolInfo& source,
    const std::unordered_set<std::string>& abstractTypes,
    std::vector<SymbolRelationInfo>& relations) {
    const std::size_t colon = code.find(':');
    const std::size_t body = code.find('{');
    if (colon == std::string::npos || body == std::string::npos || colon >= body) {
        return;
    }

    std::stringstream bases(code.substr(colon + 1, body - colon - 1));
    std::string base;
    while (std::getline(bases, base, ',')) {
        static const std::regex modifierRegex(R"(\b(public|protected|private|virtual)\b)");
        try {
            base = std::regex_replace(base, modifierRegex, " ");
        } catch (const std::regex_error&) {
            continue;
        }
        const std::string target = qualifyTypeName(base, source.qualifiedName);
        if (target.empty()) {
            continue;
        }
        const std::string unqualified = target.substr(target.rfind("::") == std::string::npos
            ? 0
            : target.rfind("::") + 2);
        const bool interfaceName = unqualified.size() > 1 && unqualified[0] == 'I' &&
            std::isupper(static_cast<unsigned char>(unqualified[1])) != 0;

        SymbolRelationInfo relation;
        relation.filePath = source.filePath;
        relation.line = source.line;
        relation.sourceQualifiedName = source.qualifiedName;
        relation.sourceStableKey = source.stableKey;
        relation.targetName = target;
        relation.kind = interfaceName || abstractTypes.find(target) != abstractTypes.end()
            ? "implements"
            : "inherits";
        relation.confidence = "heuristic";
        relations.push_back(std::move(relation));
    }
}

void extractFieldRelation(
    const std::string& code,
    const TypeScope& source,
    const std::filesystem::path& relativePath,
    std::uint64_t lineNumber,
    std::vector<SymbolRelationInfo>& relations) {
    if (code.find('(') != std::string::npos || code.find(';') == std::string::npos) {
        return;
    }
    static const std::regex fieldRegex(
        R"(^\s*(?:mutable\s+|static\s+|const\s+)*([A-Za-z_][A-Za-z0-9_:]*(?:\s*<[^;(){}]+>)?)\s*([*&]+)?\s+[A-Za-z_][A-Za-z0-9_]*\s*(?:[;={]))");
    std::smatch match;
    try {
        if (!std::regex_search(code, match, fieldRegex)) {
            return;
        }
    } catch (const std::regex_error&) {
        return;
    }

    const std::string rawTarget = trim(match[1].str());
    static const std::unordered_set<std::string> primitiveTypes = {
        "bool", "char", "double", "float", "int", "long", "short", "signed", "unsigned",
        "size_t", "std::size_t", "auto", "void"
    };
    if (primitiveTypes.find(rawTarget) != primitiveTypes.end()) {
        return;
    }

    SymbolRelationInfo relation;
    relation.filePath = relativePath;
    relation.line = lineNumber;
    relation.sourceQualifiedName = source.qualifiedName;
    relation.sourceStableKey = source.stableKey;
    relation.targetName = qualifyTypeName(rawTarget, source.qualifiedName);
    relation.kind = match[2].matched ? "uses" : "contains";
    relation.confidence = "heuristic";
    relations.push_back(std::move(relation));
}

bool containsIdentifier(const std::string& text, const std::string& identifier) {
    if (identifier.empty()) {
        return false;
    }
    std::size_t position = text.find(identifier);
    while (position != std::string::npos) {
        const bool startsClean = position == 0 ||
            !isIdentifierPart(static_cast<unsigned char>(text[position - 1]));
        const std::size_t end = position + identifier.size();
        const bool endsClean = end == text.size() ||
            !isIdentifierPart(static_cast<unsigned char>(text[end]));
        if (startsClean && endsClean) {
            return true;
        }
        position = text.find(identifier, position + 1);
    }
    return false;
}

void extractSignatureRelations(
    const std::string& code,
    const TypeScope& source,
    const std::filesystem::path& relativePath,
    std::uint64_t lineNumber,
    const std::vector<SymbolInfo>& symbols,
    std::vector<SymbolRelationInfo>& relations) {
    std::set<std::string> emitted;
    for (const SymbolInfo& candidate : symbols) {
        if ((candidate.kind != "class" && candidate.kind != "struct" && candidate.kind != "enum") ||
            candidate.qualifiedName == source.qualifiedName ||
            !emitted.insert(candidate.qualifiedName).second) {
            continue;
        }
        if (!containsIdentifier(code, candidate.name) &&
            !containsIdentifier(code, candidate.qualifiedName)) {
            continue;
        }

        SymbolRelationInfo relation;
        relation.filePath = relativePath;
        relation.line = lineNumber;
        relation.sourceQualifiedName = source.qualifiedName;
        relation.sourceStableKey = source.stableKey;
        relation.targetName = candidate.qualifiedName;
        relation.kind = "uses";
        relation.confidence = "heuristic";
        relations.push_back(std::move(relation));
    }
}

std::optional<EntryPointInfo> entryPointFor(const SymbolInfo& symbol) {
    if (!symbol.isDefinition || symbol.kind != "function") {
        return std::nullopt;
    }
    const std::string& name = symbol.name;
    std::string kind;
    if (name == "main" || name == "WinMain" || name == "wWinMain") {
        kind = "program";
    } else if (name == "DllMain") {
        kind = "library";
    } else if (name == "test_main" || name == "run_tests") {
        kind = "test";
    } else {
        return std::nullopt;
    }

    EntryPointInfo entry;
    entry.filePath = symbol.filePath;
    entry.line = symbol.line;
    entry.qualifiedName = symbol.qualifiedName;
    entry.symbolStableKey = symbol.stableKey;
    entry.kind = kind;
    entry.confidence = symbol.confidence;
    return entry;
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
    std::vector<TypeScope> typeScopes;
    std::vector<FunctionScope> functionScopes;
    std::unordered_set<std::string> abstractTypes;
    struct PendingSignature {
        std::string text;
        std::uint64_t line = 0;
        int extraLines = 0;
    };
    std::optional<PendingSignature> pendingSignature;
    bool directiveContinuation = false;

    std::string buffer;
    while (std::getline(input, buffer)) {
        ++lineNumber;
        classifyLine(buffer, insideBlockCommentForMetrics, analysis.lines);

        const std::string codeWithStrings =
            stripCommentsPreserveStrings(buffer, insideBlockCommentForAnalysis);
        const std::string code = blankStringLiteralContents(codeWithStrings);
        const std::string scope = currentQualifiedScope(namespaceScopes, typeScopes);

        try {
            extractInclude(codeWithStrings, relativePath, lineNumber, analysis.includes);
            if (!typeScopes.empty() && functionScopes.empty()) {
                extractFieldRelation(
                    code,
                    typeScopes.back(),
                    relativePath,
                    lineNumber,
                    analysis.relations);
            }
            const bool isTypeSymbol =
                extractTypeSymbol(code, relativePath, lineNumber, scope, analysis.symbols);
            if (isTypeSymbol) {
                const SymbolInfo& type = analysis.symbols.back();
                if (code.find("= 0") != std::string::npos || code.find("=0") != std::string::npos) {
                    abstractTypes.insert(type.qualifiedName);
                }
                extractBaseRelations(code, type, abstractTypes, analysis.relations);
                if (type.isDefinition) {
                    typeScopes.push_back(TypeScope{
                        type.name,
                        type.qualifiedName,
                        type.stableKey,
                        braceDepth + 1,
                    });
                }
            }

            const bool continuedDirective = directiveContinuation;
            const bool preprocessorLine = continuedDirective || isPreprocessorLine(code);
            if (preprocessorLine) {
                if (!continuedDirective) {
                    collectMacroDefine(code, analysis.macroNames);
                }
                directiveContinuation = lineContinues(code);
            } else {
                directiveContinuation = false;
            }

            const std::string trimmedLine = trim(code);
            const std::size_t openParen = trimmedLine.find('(');
            const bool lineHasTerminator =
                trimmedLine.find('{') != std::string::npos || trimmedLine.find(';') != std::string::npos;
            const std::string leadingToken = firstIdentifier(trimmedLine);
            const bool holdSignature = !isTypeSymbol && !preprocessorLine && pendingSignature == std::nullopt &&
                openParen != std::string::npos && !isControlFlowStart(leadingToken) &&
                !lineHasTerminator;

            if (pendingSignature && !preprocessorLine) {
                pendingSignature->text.push_back(' ');
                pendingSignature->text += trimmedLine;
                ++pendingSignature->extraLines;
            } else if (holdSignature) {
                pendingSignature = PendingSignature{trimmedLine, lineNumber, 0};
            }

            const bool pendingReady = pendingSignature &&
                (pendingSignature->extraLines > 6 ||
                 (findMatchingParen(pendingSignature->text, pendingSignature->text.find('(')) != std::string::npos &&
                  (pendingSignature->text.find('{') != std::string::npos ||
                   pendingSignature->text.find(';') != std::string::npos)));
            std::string signatureText = code;
            std::uint64_t signatureLine = lineNumber;
            if (pendingReady) {
                signatureText = pendingSignature->text;
                signatureLine = pendingSignature->line;
                pendingSignature.reset();
            }

            const bool parseSignature = !isTypeSymbol && !preprocessorLine &&
                (pendingReady || pendingSignature == std::nullopt);
            const ExtractedFunction function = parseSignature
                ? extractFunctionSymbol(
                      signatureText,
                      relativePath,
                      signatureLine,
                      scope,
                      functionScopes.empty(),
                      analysis.symbols)
                : ExtractedFunction{};
            if (!function.name.empty()) {
                const SymbolInfo& symbol = analysis.symbols.back();
                if (!typeScopes.empty()) {
                    extractSignatureRelations(
                        code,
                        typeScopes.back(),
                        relativePath,
                        lineNumber,
                        analysis.symbols,
                        analysis.relations);
                } else if (!symbol.parentStableKey.empty()) {
                    extractSignatureRelations(
                        code,
                        TypeScope{
                            symbol.parentQualifiedName.substr(
                                symbol.parentQualifiedName.rfind("::") == std::string::npos
                                    ? 0
                                    : symbol.parentQualifiedName.rfind("::") + 2),
                            symbol.parentQualifiedName,
                            symbol.parentStableKey,
                            braceDepth,
                        },
                        relativePath,
                        lineNumber,
                        analysis.symbols,
                        analysis.relations);
                }
                if (const std::optional<EntryPointInfo> entry = entryPointFor(symbol)) {
                    analysis.entryPoints.push_back(*entry);
                }
                if (function.hasBody) {
                    functionScopes.push_back(FunctionScope{
                        function.qualifiedName,
                        function.stableKey,
                        braceDepth + 1,
                    });
                }
            }

            const std::string callerQualifiedName = !function.name.empty()
                ? function.qualifiedName
                : (functionScopes.empty() ? std::string() : functionScopes.back().qualifiedName);
            const std::string callerStableKey = !function.name.empty()
                ? function.stableKey
                : (functionScopes.empty() ? std::string() : functionScopes.back().stableKey);
            if (!preprocessorLine) {
                extractCalls(
                    code,
                    relativePath,
                    lineNumber,
                    function.name,
                    callerQualifiedName,
                    callerStableKey,
                    analysis.calls);
            }

            static const std::regex namespaceRegex(R"(^\s*namespace\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{)");
            std::smatch namespaceMatch;
            const bool opensNamespace = std::regex_search(code, namespaceMatch, namespaceRegex);
            if (opensNamespace) {
                namespaceScopes.push_back(
                    NamespaceScope{namespaceMatch[1].str(), braceDepth + 1});
            }
        } catch (const std::regex_error& error) {
            analysis.diagnostics.push_back(
                "warning: regex skipped in " + relativePath.generic_string() + ":" +
                std::to_string(lineNumber) + " (" + error.what() + ")");
        }

        braceDepth += countCharacter(code, '{');
        braceDepth -= countCharacter(code, '}');
        if (braceDepth < 0) {
            braceDepth = 0;
        }

        while (!namespaceScopes.empty() && braceDepth < namespaceScopes.back().depth) {
            namespaceScopes.pop_back();
        }
        while (!typeScopes.empty() && braceDepth < typeScopes.back().depth) {
            typeScopes.pop_back();
        }
        while (!functionScopes.empty() && braceDepth < functionScopes.back().depth) {
            functionScopes.pop_back();
        }
    }

    return analysis;
}

template <typename T>
void appendAll(std::vector<T>& target, const std::vector<T>& source) {
    target.insert(target.end(), source.begin(), source.end());
}

void appendProjectSignatureRelations(ScanResult& result) {
    std::vector<const SymbolInfo*> types;
    for (const SymbolInfo& symbol : result.symbols) {
        if (symbol.kind == "class" || symbol.kind == "struct" || symbol.kind == "enum") {
            types.push_back(&symbol);
        }
    }
    std::set<std::string> emitted;
    for (const SymbolRelationInfo& relation : result.relations) {
        emitted.insert(
            relation.sourceStableKey + "\n" + relation.targetName + "\n" +
            std::to_string(relation.line) + "\n" + relation.kind);
    }
    for (const SymbolInfo& function : result.symbols) {
        if (function.kind != "function") {
            continue;
        }
        const SymbolInfo* source = nullptr;
        for (const SymbolInfo* candidate : types) {
            if (candidate->qualifiedName == function.parentQualifiedName) {
                source = candidate;
                break;
            }
        }
        const std::string sourceQualifiedName = source != nullptr
            ? source->qualifiedName
            : function.qualifiedName;
        const std::string sourceStableKey = source != nullptr
            ? source->stableKey
            : function.stableKey;
        for (const SymbolInfo* target : types) {
            if (target->qualifiedName == sourceQualifiedName ||
                (!containsIdentifier(function.signature, target->name) &&
                 !containsIdentifier(function.signature, target->qualifiedName))) {
                continue;
            }
            const std::string key = sourceStableKey + "\n" + target->qualifiedName + "\n" +
                std::to_string(function.line) + "\nuses";
            if (!emitted.insert(key).second) {
                continue;
            }
            SymbolRelationInfo relation;
            relation.filePath = function.filePath;
            relation.line = function.line;
            relation.sourceQualifiedName = sourceQualifiedName;
            relation.sourceStableKey = sourceStableKey;
            relation.targetName = target->qualifiedName;
            relation.kind = "uses";
            relation.confidence = function.confidence;
            result.relations.push_back(std::move(relation));
        }
    }
}

} // namespace

ScanResult ProjectScanner::scan(const std::filesystem::path& rootPath) const {
    return scan(rootPath, ScanOptions{});
}

ScanResult ProjectScanner::scan(const std::filesystem::path& rootPath, const ScanOptions& options) const {
    const auto startedAt = std::chrono::steady_clock::now();

    ScanResult result;
    result.rootPath = std::filesystem::absolute(rootPath);
    result.mode = "heuristic";
    result.confidence = "heuristic";

    if (!std::filesystem::exists(result.rootPath)) {
        throw std::runtime_error("Project path does not exist: " + result.rootPath.string());
    }

    if (!std::filesystem::is_directory(result.rootPath)) {
        throw std::runtime_error("Project path is not a directory: " + result.rootPath.string());
    }

    std::unordered_map<std::string, FileInfo> cachedFiles;
    if (!options.previousScanJson.empty()) {
        cachedFiles = ScanCache::loadPreviousFiles(options.previousScanJson, result.diagnostics);
    }

    CompileCommandsDatabase compileCommands;
    std::filesystem::path compileCommandsPath = options.compileCommandsPath;
    if (compileCommandsPath.empty()) {
        compileCommandsPath = CompileCommandsLoader::findDefault(result.rootPath);
    }
    if (!compileCommandsPath.empty() && std::filesystem::exists(compileCommandsPath)) {
        compileCommands = CompileCommandsLoader::load(compileCommandsPath);
        result.compileCommandsPath = compileCommandsPath;
        result.compileCommandsEntries = compileCommands.entries.size();
        appendAll(result.diagnostics, compileCommands.diagnostics);
        if (!compileCommands.entries.empty()) {
            result.mode = "hybrid";
            result.confidence = "hybrid";
        }
    }

    ScanProgress progress;
    std::unordered_set<std::string> definedMacroNames;
    std::filesystem::recursive_directory_iterator iterator(
        result.rootPath,
        std::filesystem::directory_options::skip_permission_denied);
    const std::filesystem::recursive_directory_iterator end;

    for (; iterator != end; ++iterator) {
        const auto& entry = *iterator;

        std::error_code statusError;
        const bool isDirectory = entry.is_directory(statusError);
        if (statusError) {
            ++result.skippedEntries;
            continue;
        }

        if (isDirectory && shouldSkipDirectory(entry.path(), options.extraSkipDirectories)) {
            iterator.disable_recursion_pending();
            continue;
        }

        if (!entry.is_regular_file(statusError) || statusError) {
            continue;
        }

        if (!isCppFile(entry.path())) {
            continue;
        }

        ++progress.filesDiscovered;

        FileInfo file;
        file.absolutePath = std::filesystem::absolute(entry.path());
        file.relativePath = std::filesystem::relative(file.absolutePath, result.rootPath);
        file.extension = normalizeExtension(file.absolutePath.extension().string());
        file.category = categorizeExtension(file.extension);
        file.mtimeUnix = fileMtimeUnix(file.absolutePath);

        std::error_code fileSizeError;
        file.sizeBytes = entry.file_size(fileSizeError);
        if (fileSizeError) {
            file.sizeBytes = 0;
            ++result.skippedEntries;
            result.diagnostics.push_back("warning: failed to read file size: " + file.relativePath.generic_string());
        }

        progress.currentFile = file.relativePath;
        const std::string cacheKey = file.relativePath.generic_string();
        const auto cachedIterator = cachedFiles.find(cacheKey);
        if (cachedIterator != cachedFiles.end() &&
            ScanCache::isReusable(cachedIterator->second, file.sizeBytes, file.mtimeUnix)) {
            FileInfo reused = cachedIterator->second;
            reused.absolutePath = file.absolutePath;
            reused.relativePath = file.relativePath;
            reused.extension = file.extension;
            reused.category = file.category;
            reused.sizeBytes = file.sizeBytes;
            reused.mtimeUnix = file.mtimeUnix;
            reused.reusedFromCache = true;
            file = std::move(reused);
            ++progress.filesReused;
            ++result.reusedFileCount;
        } else {
            const FileAnalysis analysis = analyzeFile(file.absolutePath, file.relativePath);
            file.lines = analysis.lines;
            file.includes = analysis.includes;
            file.symbols = analysis.symbols;
            file.calls = analysis.calls;
            file.relations = analysis.relations;
            file.entryPoints = analysis.entryPoints;
            definedMacroNames.insert(analysis.macroNames.begin(), analysis.macroNames.end());
            appendAll(result.diagnostics, analysis.diagnostics);
            ++progress.filesAnalyzed;
        }

        if (options.onProgress) {
            options.onProgress(progress);
        } else if (options.showProgress &&
                   (progress.filesDiscovered % 200 == 0 || progress.filesDiscovered == 1)) {
            // Progress is surfaced by CLI callback; keep default quiet here.
        }

        appendAll(result.includes, file.includes);
        appendAll(result.symbols, file.symbols);
        appendAll(result.calls, file.calls);
        appendAll(result.relations, file.relations);
        appendAll(result.entryPoints, file.entryPoints);

        result.stats.totalBytes += file.sizeBytes;
        addLineMetrics(result.stats.lines, file.lines);
        ++result.extensionCounts[file.extension];
        ++result.topLevelDirectoryCounts[topLevelName(file.relativePath)];
        ++result.secondLevelDirectoryCounts[secondLevelName(file.relativePath)];

        if (file.category == "source") {
            ++result.stats.sourceFiles;
        } else if (file.category == "header") {
            ++result.stats.headerFiles;
        } else if (file.category == "inline") {
            ++result.stats.inlineFiles;
        }

        result.files.push_back(std::move(file));
    }

    // Names introduced by #define are not calls, even when they are not ALL_CAPS.
    // Filter after the walk so a header's macro covers later call sites.
    auto dropDefinedMacroCalls = [&definedMacroNames](std::vector<CallInfo>& calls) {
        calls.erase(
            std::remove_if(calls.begin(), calls.end(), [&definedMacroNames](const CallInfo& call) {
                const std::size_t scope = call.name.rfind("::");
                const std::string leaf = scope == std::string::npos
                    ? call.name
                    : call.name.substr(scope + 2);
                return definedMacroNames.find(leaf) != definedMacroNames.end();
            }),
            calls.end());
    };
    for (FileInfo& file : result.files) {
        dropDefinedMacroCalls(file.calls);
    }
    dropDefinedMacroCalls(result.calls);

    if (!compileCommands.entries.empty()) {
        PreciseEnhancer::enhance(
            result.rootPath,
            compileCommands,
            result.includes,
            result.files,
            result.diagnostics);

        result.symbols.clear();
        result.calls.clear();
        result.relations.clear();
        result.entryPoints.clear();
        for (const FileInfo& file : result.files) {
            appendAll(result.symbols, file.symbols);
            appendAll(result.calls, file.calls);
            appendAll(result.relations, file.relations);
            appendAll(result.entryPoints, file.entryPoints);
        }
    }

    appendProjectSignatureRelations(result);

    std::sort(result.files.begin(), result.files.end(), [](const FileInfo& left, const FileInfo& right) {
        return left.relativePath.string() < right.relativePath.string();
    });

    const auto finishedAt = std::chrono::steady_clock::now();
    result.scanDurationMs = static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::milliseconds>(finishedAt - startedAt).count());

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

bool ProjectScanner::shouldSkipDirectory(
    const std::filesystem::path& path,
    const std::vector<std::string>& extraSkipDirectories) {
    static const std::unordered_set<std::string> names = {
        ".git", ".svn", ".hg", ".cppinsight",
        "build", "build-codex", "cmake-build-debug", "cmake-build-release",
        "node_modules", ".cache", ".vs", ".idea", ".vscode", "target", "dist",
        ".venv", "venv", "env", ".tox", "__pycache__", "site-packages",
        ".pytest_cache", ".mypy_cache", "Pods", "DerivedData"
    };

    const std::string filename = path.filename().string();
    if (names.find(filename) != names.end()) {
        return true;
    }

    return std::find(extraSkipDirectories.begin(), extraSkipDirectories.end(), filename) !=
        extraSkipDirectories.end();
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
