#pragma once

#include <string>

namespace sample {

class Worker {
public:
    explicit Worker(std::string owner);

    std::string process(int jobs) const;

private:
    std::string owner_;
};

}  // namespace sample
