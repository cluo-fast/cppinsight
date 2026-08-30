#include "project/CompileCommands.h"
#include "project/FileInfo.h"

#include <algorithm>
#include <cctype>
#include <fstream>
#include <sstream>

namespace cppinsight {
namespace {

std::string readFile(const std::filesystem::path& path) {
    std::ifstream input(path, std::ios::binary);
    if (!input) {
        return {};
    }
    std::ostringstream buffer;
    buffer << input.rdbuf();
    return buffer.str();
}

std::string unescapeJsonString(const std::string& value) {
    std::string output;
    output.reserve(value.size());
    for (std::size_t index = 0; index < value.size(); ++index) {
        const char character = value[index];
        if (character != '\\' || index + 1 >= value.size()) {
            output.push_back(character);
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

bool extractJsonStringField(
    const std::string& objectText,
    const std::string& fieldName,
    std::string& value) {
    const std::string pattern = "\"" + fieldName + "\"";
    std::size_t cursor = objectText.find(pattern);
    if (cursor == std::string::npos) {
        return false;
    }

    cursor = objectText.find(':', cursor + pattern.size());
    if (cursor == std::string::npos) {
        return false;
    }

    cursor = objectText.find('"', cursor + 1);
    if (cursor == std::string::npos) {
        return false;
    }

    std::size_t end = cursor + 1;
    while (end < objectText.size()) {
        if (objectText[end] == '\\') {
            end += 2;
            continue;
        }
        if (objectText[end] == '"') {
            value = unescapeJsonString(objectText.substr(cursor + 1, end - cursor - 1));
            return true;
        }
        ++end;
    }

    return false;
}

std::vector<std::string> extractJsonStringArrayField(
    const std::string& objectText,
    const std::string& fieldName) {
    const std::string pattern = "\"" + fieldName + "\"";
    std::size_t cursor = objectText.find(pattern);
    if (cursor == std::string::npos) {
        return {};
    }

    cursor = objectText.find('[', cursor + pattern.size());
    if (cursor == std::string::npos) {
        return {};
    }

    std::vector<std::string> values;
    ++cursor;
    while (cursor < objectText.size()) {
        while (cursor < objectText.size() &&
               (std::isspace(static_cast<unsigned char>(objectText[cursor])) != 0 ||
                objectText[cursor] == ',')) {
            ++cursor;
        }
        if (cursor >= objectText.size() || objectText[cursor] == ']') {
            break;
        }
        if (objectText[cursor] != '"') {
            return {};
        }

        const std::size_t start = ++cursor;
        std::size_t end = start;
        while (end < objectText.size()) {
            if (objectText[end] == '\\') {
                end += 2;
                continue;
            }
            if (objectText[end] == '"') {
                break;
            }
            ++end;
        }
        if (end >= objectText.size()) {
            return {};
        }
        values.push_back(unescapeJsonString(objectText.substr(start, end - start)));
        cursor = end + 1;
    }
    return values;
}

std::string joinCommandArguments(const std::vector<std::string>& arguments) {
    std::string command;
    for (const std::string& argument : arguments) {
        if (!command.empty()) {
            command.push_back(' ');
        }
        const bool needsQuotes = argument.find_first_of(" \t") != std::string::npos;
        if (needsQuotes) {
            command.push_back('"');
        }
        command += argument;
        if (needsQuotes) {
            command.push_back('"');
        }
    }
    return command;
}

std::vector<std::string> splitCommand(const std::string& command) {
    std::vector<std::string> tokens;
    std::string current;
    bool inQuotes = false;

    for (std::size_t index = 0; index < command.size(); ++index) {
        const char character = command[index];
        if (character == '"') {
            inQuotes = !inQuotes;
            continue;
        }
        if (!inQuotes && std::isspace(static_cast<unsigned char>(character)) != 0) {
            if (!current.empty()) {
                tokens.push_back(current);
                current.clear();
            }
            continue;
        }
        current.push_back(character);
    }

    if (!current.empty()) {
        tokens.push_back(current);
    }

    return tokens;
}

std::vector<std::filesystem::path> extractIncludePaths(
    const std::string& command,
    const std::filesystem::path& directory) {
    std::vector<std::filesystem::path> includePaths;
    const std::vector<std::string> tokens = splitCommand(command);

    for (std::size_t index = 0; index < tokens.size(); ++index) {
        const std::string& token = tokens[index];
        std::string includeValue;

        if (token.rfind("-I", 0) == 0 && token.size() > 2) {
            includeValue = token.substr(2);
        } else if ((token == "-I" || token == "-isystem" || token == "/I") && index + 1 < tokens.size()) {
            includeValue = tokens[++index];
        } else if (token.rfind("/I", 0) == 0 && token.size() > 2) {
            includeValue = token.substr(2);
        } else if (token.rfind("-isystem", 0) == 0 && token.size() > 8) {
            includeValue = token.substr(8);
        }

        if (includeValue.empty()) {
            continue;
        }

        std::filesystem::path includePath = includeValue;
        if (includePath.is_relative()) {
            includePath = directory / includePath;
        }
        includePaths.push_back(std::filesystem::weakly_canonical(includePath));
    }

    return includePaths;
}

std::string normalizePathKey(const std::filesystem::path& path) {
    std::filesystem::path normalized = std::filesystem::weakly_canonical(path);
    std::string value = normalized.generic_string();
    std::transform(value.begin(), value.end(), value.begin(), [](unsigned char character) {
        return static_cast<char>(std::tolower(character));
    });
    return value;
}

} // namespace

CompileCommandsDatabase CompileCommandsLoader::load(const std::filesystem::path& path) {
    CompileCommandsDatabase database;
    const std::string text = readFile(path);
    if (text.empty()) {
        database.diagnostics.push_back("warning: failed to read compile_commands.json: " + path.string());
        return database;
    }

    std::size_t cursor = 0;
    while (cursor < text.size()) {
        const std::size_t objectBegin = text.find('{', cursor);
        if (objectBegin == std::string::npos) {
            break;
        }

        int depth = 0;
        std::size_t objectEnd = objectBegin;
        for (; objectEnd < text.size(); ++objectEnd) {
            if (text[objectEnd] == '{') {
                ++depth;
            } else if (text[objectEnd] == '}') {
                --depth;
                if (depth == 0) {
                    break;
                }
            }
        }

        if (objectEnd >= text.size()) {
            break;
        }

        const std::string objectText = text.substr(objectBegin, objectEnd - objectBegin + 1);
        cursor = objectEnd + 1;

        std::string fileValue;
        std::string directoryValue;
        std::string commandValue;
        if (!extractJsonStringField(objectText, "file", fileValue)) {
            continue;
        }
        extractJsonStringField(objectText, "directory", directoryValue);
        const bool hasCommand = extractJsonStringField(objectText, "command", commandValue);
        if (!hasCommand || commandValue.empty()) {
            const std::vector<std::string> arguments =
                extractJsonStringArrayField(objectText, "arguments");
            if (!arguments.empty()) {
                commandValue = joinCommandArguments(arguments);
            } else if (!hasCommand) {
                // Keep accepting the historical string form used by a few
                // generators even though the compile_commands standard calls
                // for an array here.
                extractJsonStringField(objectText, "arguments", commandValue);
            }
        }

        CompileCommandEntry entry;
        entry.directory = directoryValue.empty() ? path.parent_path() : std::filesystem::path(directoryValue);
        if (entry.directory.is_relative()) {
            entry.directory = path.parent_path() / entry.directory;
        }
        entry.directory = std::filesystem::weakly_canonical(entry.directory);
        entry.file = std::filesystem::path(fileValue);
        if (entry.file.is_relative()) {
            entry.file = entry.directory / entry.file;
        }
        entry.file = std::filesystem::weakly_canonical(entry.file);
        entry.includePaths = extractIncludePaths(commandValue, entry.directory);

        database.compiledFiles.insert(normalizePathKey(entry.file));
        for (const std::filesystem::path& includePath : entry.includePaths) {
            database.allIncludePaths.push_back(includePath);
        }
        database.entries.push_back(std::move(entry));
    }

    std::sort(database.allIncludePaths.begin(), database.allIncludePaths.end());
    database.allIncludePaths.erase(
        std::unique(database.allIncludePaths.begin(), database.allIncludePaths.end()),
        database.allIncludePaths.end());

    if (database.entries.empty()) {
        database.diagnostics.push_back("warning: no compile commands parsed from " + path.string());
    }

    return database;
}

std::filesystem::path CompileCommandsLoader::findDefault(const std::filesystem::path& projectRoot) {
    const std::filesystem::path candidates[] = {
        projectRoot / "compile_commands.json",
        projectRoot / "build" / "compile_commands.json",
        projectRoot / "build-codex" / "compile_commands.json",
        projectRoot / "out" / "compile_commands.json",
    };

    for (const std::filesystem::path& candidate : candidates) {
        if (std::filesystem::exists(candidate)) {
            return candidate;
        }
    }

    return {};
}

void PreciseEnhancer::enhance(
    const std::filesystem::path& projectRoot,
    const CompileCommandsDatabase& database,
    std::vector<IncludeInfo>& includes,
    std::vector<FileInfo>& files,
    std::vector<std::string>& diagnostics) {
    if (database.entries.empty()) {
        return;
    }

    for (FileInfo& file : files) {
        const std::string absoluteKey = normalizePathKey(file.absolutePath);
        if (database.compiledFiles.find(absoluteKey) != database.compiledFiles.end()) {
            file.fromCompileCommands = true;
            for (SymbolInfo& symbol : file.symbols) {
                symbol.confidence = "precise";
            }
            for (CallInfo& call : file.calls) {
                call.confidence = "precise";
            }
        }
    }

    std::uint64_t resolvedCount = 0;
    for (IncludeInfo& include : includes) {
        if (include.kind != "local" || include.resolved) {
            continue;
        }

        const std::filesystem::path includerAbsolute =
            projectRoot / include.filePath;
        const std::filesystem::path localCandidate =
            std::filesystem::weakly_canonical(includerAbsolute.parent_path() / include.target);

        std::vector<std::filesystem::path> searchRoots = database.allIncludePaths;
        searchRoots.insert(searchRoots.begin(), includerAbsolute.parent_path());
        searchRoots.push_back(projectRoot);

        for (const std::filesystem::path& root : searchRoots) {
            const std::filesystem::path candidate = std::filesystem::weakly_canonical(root / include.target);
            if (!std::filesystem::exists(candidate)) {
                continue;
            }

            include.resolved = true;
            include.resolvedPath = std::filesystem::relative(candidate, projectRoot).generic_string();
            ++resolvedCount;
            break;
        }

        if (!include.resolved && std::filesystem::exists(localCandidate)) {
            include.resolved = true;
            include.resolvedPath = std::filesystem::relative(localCandidate, projectRoot).generic_string();
            ++resolvedCount;
        }
    }

    for (FileInfo& file : files) {
        for (IncludeInfo& include : file.includes) {
            if (include.kind != "local" || include.resolved) {
                continue;
            }
            for (const IncludeInfo& globalInclude : includes) {
                if (globalInclude.filePath == include.filePath &&
                    globalInclude.line == include.line &&
                    globalInclude.target == include.target &&
                    globalInclude.resolved) {
                    include.resolved = true;
                    include.resolvedPath = globalInclude.resolvedPath;
                    break;
                }
            }
        }
    }

    diagnostics.push_back(
        "info: precise enhance resolved " + std::to_string(resolvedCount) +
        " local includes using compile_commands (" +
        std::to_string(database.entries.size()) + " entries)");
}

} // namespace cppinsight
