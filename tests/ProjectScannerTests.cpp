#include "project/CompileCommands.h"
#include "project/ProjectScanner.h"
#include "report/JsonReportWriter.h"
#include "report/OverviewWriter.h"

#include <chrono>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>

namespace {

void require(bool condition, const std::string& message) {
    if (!condition) {
        throw std::runtime_error(message);
    }
}

void writeFile(const std::filesystem::path& path, const std::string& content) {
    std::filesystem::create_directories(path.parent_path());
    std::ofstream output(path);
    require(static_cast<bool>(output), "failed to create test file: " + path.string());
    output << content;
}

std::filesystem::path makeFixtureRoot() {
    const auto stamp = std::chrono::steady_clock::now().time_since_epoch().count();
    std::filesystem::path root = std::filesystem::temp_directory_path() /
        ("cppinsight_tests_" + std::to_string(stamp));

    writeFile(root / "src" / "main.cpp",
        "#include \"widget.h\"\n"
        "#include <vector>\n"
        "\n"
        "namespace app {\n"
        "class Runner {\n"
        "public:\n"
        "    void run();\n"
        "};\n"
        "\n"
        "void Runner::run() {\n"
        "    helper();\n"
        "}\n"
        "}\n"
        "\n"
        "int main() {\n"
        "    app::Runner runner;\n"
        "    Widget widget(1);\n"
        "    runner.run();\n"
        "}\n");

    writeFile(root / "include" / "widget.h",
        "#pragma once\n"
        "struct Widget {};\n"
        "int helper();\n");

    writeFile(root / "build" / "generated.cpp",
        "int generated() { return 0; }\n");

    writeFile(root / "notes.txt",
        "not C++\n");

    return root;
}

bool hasInclude(const cppinsight::ScanResult& result, const std::string& target, const std::string& kind) {
    for (const cppinsight::IncludeInfo& include : result.includes) {
        if (include.target == target && include.kind == kind) {
            return true;
        }
    }
    return false;
}

bool hasSymbol(const cppinsight::ScanResult& result, const std::string& name, const std::string& kind) {
    for (const cppinsight::SymbolInfo& symbol : result.symbols) {
        if (symbol.name == name && symbol.kind == kind) {
            return true;
        }
    }
    return false;
}

bool hasCall(const cppinsight::ScanResult& result, const std::string& name) {
    for (const cppinsight::CallInfo& call : result.calls) {
        if (call.name == name) {
            return true;
        }
    }
    return false;
}

const cppinsight::SymbolInfo* findQualifiedSymbol(
    const cppinsight::ScanResult& result,
    const std::string& qualifiedName) {
    for (const cppinsight::SymbolInfo& symbol : result.symbols) {
        if (symbol.qualifiedName == qualifiedName) {
            return &symbol;
        }
    }
    return nullptr;
}

bool hasRelation(
    const cppinsight::ScanResult& result,
    const std::string& source,
    const std::string& target,
    const std::string& kind) {
    for (const cppinsight::SymbolRelationInfo& relation : result.relations) {
        if (relation.sourceQualifiedName == source &&
            relation.targetName == target &&
            relation.kind == kind &&
            !relation.filePath.empty() &&
            relation.line > 0) {
            return true;
        }
    }
    return false;
}

bool hasOwnedCall(
    const cppinsight::ScanResult& result,
    const std::string& caller,
    const std::string& target) {
    for (const cppinsight::CallInfo& call : result.calls) {
        if (call.callerQualifiedName == caller &&
            call.name == target &&
            !call.callerStableKey.empty()) {
            return true;
        }
    }
    return false;
}

bool hasEntry(
    const cppinsight::ScanResult& result,
    const std::string& qualifiedName,
    const std::string& kind) {
    for (const cppinsight::EntryPointInfo& entry : result.entryPoints) {
        if (entry.qualifiedName == qualifiedName &&
            entry.kind == kind &&
            !entry.symbolStableKey.empty() &&
            !entry.filePath.empty() &&
            entry.line > 0) {
            return true;
        }
    }
    return false;
}

std::map<std::string, std::string> stableKeysByQualifiedName(
    const cppinsight::ScanResult& result) {
    std::map<std::string, std::string> keys;
    for (const cppinsight::SymbolInfo& symbol : result.symbols) {
        if (!symbol.qualifiedName.empty()) {
            keys[symbol.qualifiedName + "\n" + symbol.signature] = symbol.stableKey;
        }
    }
    return keys;
}

void testScannerFindsProjectInventory() {
    const std::filesystem::path root = makeFixtureRoot();

    cppinsight::ProjectScanner scanner;
    const cppinsight::ScanResult result = scanner.scan(root);

    require(result.files.size() == 2, "scanner should index only C/C++ files outside skipped dirs");
    require(result.stats.sourceFiles == 1, "scanner should count one source file");
    require(result.stats.headerFiles == 1, "scanner should count one header file");
    require(result.stats.lines.code > 0, "scanner should count code lines");
    require(result.extensionCounts.at(".cpp") == 1, "scanner should count .cpp extension");
    require(result.extensionCounts.at(".h") == 1, "scanner should count .h extension");
    require(result.mode == "heuristic", "default mode should be heuristic");
    require(!result.secondLevelDirectoryCounts.empty(), "scanner should populate second-level layout");

    require(hasInclude(result, "widget.h", "local"), "scanner should extract local include");
    require(hasInclude(result, "vector", "system"), "scanner should extract system include");
    require(hasSymbol(result, "Runner", "class"), "scanner should extract class symbol");
    require(hasSymbol(result, "Widget", "struct"), "scanner should extract struct symbol");
    require(hasSymbol(result, "main", "function"), "scanner should extract function symbol");
    require(hasCall(result, "helper"), "scanner should extract simple call clue");
    for (const cppinsight::CallInfo& call : result.calls) {
        require(!call.callerQualifiedName.empty(), "call clues should belong to a function body");
    }
    require(!hasCall(result, "widget"), "local variable construction should not be a call clue");

    std::filesystem::remove_all(root);
}

void testOverviewDetectsEntryAndModules() {
    const std::filesystem::path root = makeFixtureRoot();
    cppinsight::ProjectScanner scanner;
    const cppinsight::ScanResult result = scanner.scan(root);

    const cppinsight::OverviewWriter writer;
    const cppinsight::OverviewData overview = writer.build(result);
    require(!overview.entryPoints.empty(), "overview should detect main entry");
    require(overview.entryPoints.front().name == "main", "entry candidate should be main");
    require(!overview.modules.empty(), "overview should include modules");
    require(!overview.hotspots.empty(), "overview should include hotspots");

    std::ostringstream markdown;
    writer.writeMarkdown(result, markdown);
    const std::string text = markdown.str();
    require(text.find("## 1. Snapshot") != std::string::npos, "overview markdown should include snapshot");
    require(text.find("## 3. Entry Point Candidates") != std::string::npos, "overview markdown should include entries");
    require(text.find("## 7. Limits") != std::string::npos, "overview markdown should include limits");

    std::filesystem::remove_all(root);
}

void testIncrementalReuse() {
    const std::filesystem::path root = makeFixtureRoot();
    cppinsight::ProjectScanner scanner;
    const cppinsight::ScanResult first = scanner.scan(root);

    const std::filesystem::path jsonPath = root / ".cppinsight" / "scan.json";
    cppinsight::JsonReportWriter().writeFile(first, jsonPath);

    cppinsight::ScanOptions options;
    options.previousScanJson = jsonPath;
    options.showProgress = false;
    const cppinsight::ScanResult second = scanner.scan(root, options);
    require(second.reusedFileCount == 2, "incremental scan should reuse unchanged files");

    std::filesystem::remove_all(root);
}

void testCompileCommandsHybridMode() {
    const std::filesystem::path root = makeFixtureRoot();
    const std::filesystem::path compileCommands = root / "compile_commands.json";
    const std::string mainPath = (root / "src" / "main.cpp").string();
    std::ofstream output(compileCommands);
    output << "[\n"
           << "  {\n"
           << "    \"directory\": \"" << root.generic_string() << "\",\n"
           << "    \"command\": \"cl.exe /Iinclude /c src/main.cpp\",\n"
           << "    \"file\": \"" << std::filesystem::path(mainPath).generic_string() << "\"\n"
           << "  }\n"
           << "]\n";
    output.close();

    cppinsight::ScanOptions options;
    options.compileCommandsPath = compileCommands;
    options.showProgress = false;
    const cppinsight::ScanResult result = cppinsight::ProjectScanner().scan(root, options);
    require(result.mode == "hybrid", "compile_commands should enable hybrid mode");
    require(result.compileCommandsEntries >= 1, "should parse compile commands");

    bool foundResolved = false;
    for (const cppinsight::IncludeInfo& include : result.includes) {
        if (include.target == "widget.h" && include.resolved) {
            foundResolved = true;
        }
    }
    require(foundResolved, "precise enhance should resolve local include");

    std::filesystem::remove_all(root);
}

void testCompileCommandsRelativeDirectoryIsAnchoredToDatabase() {
    const std::filesystem::path root = makeFixtureRoot();
    const std::filesystem::path compileCommands = root / "compile_commands.json";
    std::ofstream output(compileCommands);
    output << "[\n"
           << "  {\n"
           << "    \"directory\": \".\",\n"
           << "    \"command\": \"c++ -Iinclude -c src/main.cpp\",\n"
           << "    \"file\": \"src/main.cpp\"\n"
           << "  }\n"
           << "]\n";
    output.close();

    cppinsight::ScanOptions options;
    options.compileCommandsPath = compileCommands;
    options.showProgress = false;
    const cppinsight::ScanResult result = cppinsight::ProjectScanner().scan(root, options);
    require(result.mode == "hybrid", "relative compile_commands should enable hybrid mode");

    bool foundResolved = false;
    for (const cppinsight::IncludeInfo& include : result.includes) {
        if (include.target == "widget.h" && include.resolved) {
            foundResolved = true;
        }
    }
    require(foundResolved, "relative compile_commands directory should resolve local include");

    std::filesystem::remove_all(root);
}

void testJsonWriterEscapesAndEmitsAnalysisContract() {
    cppinsight::ScanResult result;
    result.rootPath = "quote\"root";
    result.mode = "heuristic";
    result.confidence = "heuristic";
    result.diagnostics.push_back("bad \"thing\"\nnext");

    cppinsight::FileInfo file;
    file.relativePath = "src/main.cpp";
    file.extension = ".cpp";
    file.category = "source";
    result.files.push_back(file);

    cppinsight::JsonReportWriter writer;
    std::ostringstream output;
    writer.write(result, output);
    const std::string json = output.str();

    require(json.find("\"schema_version\": 3") != std::string::npos, "JSON should include schema version 3");
    require(json.find("\"generator_version\": \"cppinsight-core/0.1.0\"") != std::string::npos,
            "JSON should include the scanner generator version");
    require(json.find("\"mode\": \"heuristic\"") != std::string::npos, "JSON should include mode");
    require(json.find("\"confidence\"") != std::string::npos, "JSON should include confidence");
    require(json.find("\"second_level_layout\"") != std::string::npos, "JSON should include second_level_layout");
    require(json.find("\"skipped_entries\"") != std::string::npos, "JSON should include skipped_entries");
    require(json.find("\"includes\"") != std::string::npos, "JSON should include includes");
    require(json.find("\"symbols\"") != std::string::npos, "JSON should include symbols");
    require(json.find("\"calls\"") != std::string::npos, "JSON should include calls");
    require(json.find("\"relations\"") != std::string::npos, "JSON should include symbol relations");
    require(json.find("\"entry_points\"") != std::string::npos, "JSON should include entry points");
    require(json.find("bad \\\"thing\\\"\\nnext") != std::string::npos, "JSON should escape diagnostics");

    // GBK-like bytes must become \\u00xx escapes so the JSON file is valid UTF-8.
    result.diagnostics.clear();
    result.diagnostics.push_back(std::string("\xC4\xE3\xBA\xC3", 4)); // "你好" in GBK
    std::ostringstream utf8Safe;
    writer.write(result, utf8Safe);
    const std::string safeJson = utf8Safe.str();
    require(safeJson.find("\\u00c4\\u00e3\\u00ba\\u00c3") != std::string::npos,
            "JSON should escape non-UTF-8 bytes");
    require(safeJson.find("\xC4\xE3") == std::string::npos,
            "JSON must not embed raw non-UTF-8 bytes");
}

void testScannerEmitsStableOwnedGraphFacts() {
    const auto stamp = std::chrono::steady_clock::now().time_since_epoch().count();
    const std::filesystem::path root =
        std::filesystem::temp_directory_path() / ("cppinsight_graph_facts_" + std::to_string(stamp));
    const std::filesystem::path source = root / "src" / "main.cpp";

    const std::string code =
        "namespace app {\n"
        "struct Base {};\n"
        "struct Widget {};\n"
        "struct IService { virtual void execute() = 0; };\n"
        "class Runner : public Base, public IService {\n"
        "    Widget widget_;\n"
        "    Widget* borrowed_;\n"
        "public:\n"
        "    void run();\n"
        "    Widget build(const Base& base);\n"
        "    void overloaded(int value);\n"
        "    void overloaded(double value);\n"
        "};\n"
        "void Runner::run() {\n"
        "    helper();\n"
        "    app::helper();\n"
        "}\n"
        "}\n"
        "int main() {\n"
        "    app::Runner runner;\n"
        "    runner.run();\n"
        "}\n";
    writeFile(source, code);

    const cppinsight::ScanResult first = cppinsight::ProjectScanner().scan(root);
    const cppinsight::SymbolInfo* runner = findQualifiedSymbol(first, "app::Runner");
    require(runner != nullptr, "Runner should have a qualified name");
    require(runner->stableKey.rfind("sym:", 0) == 0, "symbol should have a stable key prefix");
    require(runner->stableKey.size() == 20, "stable key should contain 16 hexadecimal digits");
    require(hasRelation(first, "app::Runner", "app::Base", "inherits"),
            "class base should emit inheritance evidence");
    require(hasRelation(first, "app::Runner", "app::IService", "implements"),
            "interface-like base should emit implements evidence");
    require(hasRelation(first, "app::Runner", "app::Widget", "contains"),
            "value field should emit composition evidence");
    require(hasRelation(first, "app::Runner", "app::Widget", "uses"),
            "pointer field should emit uses evidence");
    require(hasRelation(first, "app::Runner", "app::Base", "uses"),
            "method signature should emit a class uses relation");
    require(hasOwnedCall(first, "app::Runner::run", "helper"),
            "call should retain its enclosing function");
    require(hasOwnedCall(first, "app::Runner::run", "app::helper"),
            "qualified call should retain its complete target name");
    require(hasOwnedCall(first, "main", "run"),
            "member call inside an entry point should retain main ownership");
    require(hasEntry(first, "main", "program"), "main should be a program entry fact");

    const std::map<std::string, std::string> firstKeys = stableKeysByQualifiedName(first);
    writeFile(source, "\n" + code);
    const cppinsight::ScanResult shifted = cppinsight::ProjectScanner().scan(root);
    require(stableKeysByQualifiedName(shifted) == firstKeys,
            "stable keys should not change after a harmless line shift");

    const std::filesystem::path jsonPath = root / ".cppinsight" / "scan.json";
    cppinsight::JsonReportWriter().writeFile(shifted, jsonPath);
    cppinsight::ScanOptions options;
    options.previousScanJson = jsonPath;
    options.showProgress = false;
    const cppinsight::ScanResult cached = cppinsight::ProjectScanner().scan(root, options);
    require(hasOwnedCall(cached, "app::Runner::run", "helper"),
            "incremental cache should preserve call ownership");
    require(hasRelation(cached, "app::Runner", "app::Base", "inherits"),
            "incremental cache should preserve symbol relations");

    std::filesystem::remove_all(root);
}

} // namespace

void testScannerSurvivesPathologicalRegexLines() {
    const auto stamp = std::chrono::steady_clock::now().time_since_epoch().count();
    const std::filesystem::path root =
        std::filesystem::temp_directory_path() / ("cppinsight_regex_" + std::to_string(stamp));

    // Nested templates + huge declarator used to trip MSVC regex_error(error_complexity).
    std::string toxic = "template <";
    for (int index = 0; index < 80; ++index) {
        toxic += "typename T";
        toxic += std::to_string(index);
        toxic += ", ";
    }
    toxic += "typename Z> inline decltype(auto) "
             "ns1::ns2::ns3::VeryLongClassName::veryLongMethodName(";
    for (int index = 0; index < 40; ++index) {
        toxic += "const std::map<std::string, std::vector<int>>& arg";
        toxic += std::to_string(index);
        toxic += ", ";
    }
    toxic += "int last) const noexcept -> decltype(last) { helper(); }\n";

    writeFile(root / "src" / "toxic.cpp", toxic + "int main() { return 0; }\n");

    cppinsight::ProjectScanner scanner;
    const cppinsight::ScanResult result = scanner.scan(root);
    require(result.files.size() == 1, "pathological file should still be indexed");
    require(hasSymbol(result, "main", "function"), "main should still be extracted");
    require(hasSymbol(result, "veryLongMethodName", "function") || hasCall(result, "helper"),
            "pathological function line should be parsed without aborting");

    std::filesystem::remove_all(root);
}

void testScannerFiltersConstructionNoise() {
    const std::filesystem::path root = std::filesystem::temp_directory_path() /
        ("cppinsight_call_noise_" + std::to_string(
            std::chrono::steady_clock::now().time_since_epoch().count()));
    writeFile(root / "src" / "main.cpp",
        "#include <string>\n"
        "class Demo {\n"
        "public:\n"
        "    explicit Demo(std::string value);\n"
        "    void run();\n"
        "private:\n"
        "    std::string value_;\n"
        "};\n"
        "Demo::Demo(std::string value) : value_(std::move(value)) {}\n"
        "void Demo::run() { std::string other(\"x\"); other.size(); }\n"
        "int main() { Demo demo(\"x\"); demo.run(); }\n");

    const cppinsight::ScanResult result = cppinsight::ProjectScanner().scan(root);
    require(!hasCall(result, "value_"), "member initializer should not be a call clue");
    require(!hasCall(result, "demo"), "local construction should not be a call clue");
    require(!hasCall(result, "other"), "local string construction should not be a call clue");
    require(hasCall(result, "std::move"), "qualified standard-library call should remain a clue");
    require(hasCall(result, "size"), "member call should remain a clue");
    require(hasCall(result, "run"), "member function call should remain a clue");

    std::filesystem::remove_all(root);
}

int main() {
    try {
        testScannerFindsProjectInventory();
        testOverviewDetectsEntryAndModules();
        testIncrementalReuse();
        testCompileCommandsHybridMode();
        testCompileCommandsRelativeDirectoryIsAnchoredToDatabase();
        testJsonWriterEscapesAndEmitsAnalysisContract();
        testScannerEmitsStableOwnedGraphFacts();
        testScannerFiltersConstructionNoise();
        testScannerSurvivesPathologicalRegexLines();
    } catch (const std::exception& exception) {
        std::cerr << exception.what() << '\n';
        return 1;
    }

    return 0;
}
