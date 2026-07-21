#include "project/ProjectScanner.h"
#include "report/JsonReportWriter.h"

#include <chrono>
#include <filesystem>
#include <fstream>
#include <iostream>
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

    require(hasInclude(result, "widget.h", "local"), "scanner should extract local include");
    require(hasInclude(result, "vector", "system"), "scanner should extract system include");
    require(hasSymbol(result, "Runner", "class"), "scanner should extract class symbol");
    require(hasSymbol(result, "Widget", "struct"), "scanner should extract struct symbol");
    require(hasSymbol(result, "main", "function"), "scanner should extract function symbol");
    require(hasCall(result, "helper"), "scanner should extract simple call clue");

    std::filesystem::remove_all(root);
}

void testJsonWriterEscapesAndEmitsAnalysisContract() {
    cppinsight::ScanResult result;
    result.rootPath = "quote\"root";
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

    require(json.find("\"schema_version\": 1") != std::string::npos, "JSON should include schema version");
    require(json.find("\"skipped_entries\"") != std::string::npos, "JSON should include skipped_entries");
    require(json.find("\"includes\"") != std::string::npos, "JSON should include includes");
    require(json.find("\"symbols\"") != std::string::npos, "JSON should include symbols");
    require(json.find("\"calls\"") != std::string::npos, "JSON should include calls");
    require(json.find("bad \\\"thing\\\"\\nnext") != std::string::npos, "JSON should escape diagnostics");
}

} // namespace

int main() {
    try {
        testScannerFindsProjectInventory();
        testJsonWriterEscapesAndEmitsAnalysisContract();
    } catch (const std::exception& exception) {
        std::cerr << exception.what() << '\n';
        return 1;
    }

    return 0;
}
