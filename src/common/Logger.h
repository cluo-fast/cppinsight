#pragma once

#include <string>

namespace cppinsight {

enum class LogLevel {
    Info,
    Warning,
    Error
};

class Logger {
public:
    static void info(const std::string& message);
    static void warning(const std::string& message);
    static void error(const std::string& message);

private:
    static void write(LogLevel level, const std::string& message);
};

} // namespace cppinsight
