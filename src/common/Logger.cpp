#include "common/Logger.h"

#include <iostream>

namespace cppinsight {

void Logger::info(const std::string& message) {
    write(LogLevel::Info, message);
}

void Logger::warning(const std::string& message) {
    write(LogLevel::Warning, message);
}

void Logger::error(const std::string& message) {
    write(LogLevel::Error, message);
}

void Logger::write(LogLevel level, const std::string& message) {
    const char* label = "INFO";
    std::ostream* stream = &std::cout;

    switch (level) {
    case LogLevel::Info:
        label = "INFO";
        stream = &std::cout;
        break;
    case LogLevel::Warning:
        label = "WARN";
        stream = &std::cout;
        break;
    case LogLevel::Error:
        label = "ERROR";
        stream = &std::cerr;
        break;
    }

    (*stream) << "[" << label << "] " << message << '\n';
}

} // namespace cppinsight
