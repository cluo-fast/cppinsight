#pragma once

#include <string>

namespace sample {

class Engine {
public:
    explicit Engine(std::string name);

    std::string run(int jobs) const;

private:
    std::string name_;
};

}  // namespace sample
