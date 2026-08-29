#include "sample/worker.hpp"

#include <sstream>
#include <utility>

namespace sample {

Worker::Worker(std::string owner) : owner_(std::move(owner)) {}

std::string Worker::process(int jobs) const {
    std::ostringstream output;
    output << owner_ << " processed " << jobs << " jobs";
    return output.str();
}

}  // namespace sample
