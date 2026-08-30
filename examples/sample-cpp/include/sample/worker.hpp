#pragma once

#include <string>

namespace sample {

class IProcessor {
public:
    virtual ~IProcessor() = default;
    virtual std::string process(int jobs) const = 0;
};

class Worker : public IProcessor {
public:
    explicit Worker(std::string owner);

    std::string process(int jobs) const override;

private:
    std::string owner_;
};

}  // namespace sample
