cmake_minimum_required(VERSION 3.16)
project(cppinsight_sample LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_executable(cppinsight_sample
    src/main.cpp
    src/engine.cpp
    src/worker.cpp
)
target_include_directories(cppinsight_sample PRIVATE include)
