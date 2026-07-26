cmake_minimum_required(VERSION 3.16)

project(CppInsightAI VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_library(cppinsight_core
    src/common/Logger.cpp
    src/project/ProjectScanner.cpp
    src/project/CompileCommands.cpp
    src/project/ScanCache.cpp
    src/report/JsonReportWriter.cpp
    src/report/OverviewWriter.cpp
)

target_include_directories(cppinsight_core PUBLIC
    src
)

add_executable(cppinsight
    src/main.cpp
)

target_link_libraries(cppinsight PRIVATE
    cppinsight_core
)

function(cppinsight_apply_warnings target_name)
if (MSVC)
    target_compile_options(${target_name} PRIVATE /W4 /utf-8)
else()
    target_compile_options(${target_name} PRIVATE -Wall -Wextra -Wpedantic)
endif()
endfunction()

cppinsight_apply_warnings(cppinsight_core)
cppinsight_apply_warnings(cppinsight)

include(CTest)

if (BUILD_TESTING)
    add_executable(cppinsight_tests
        tests/ProjectScannerTests.cpp
    )

    target_link_libraries(cppinsight_tests PRIVATE
        cppinsight_core
    )

    cppinsight_apply_warnings(cppinsight_tests)

    add_test(NAME cppinsight_tests COMMAND cppinsight_tests)
endif()
