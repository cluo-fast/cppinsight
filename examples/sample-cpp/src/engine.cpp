#include "sample/engine.hpp"
#include "sample/worker.hpp"

#include <utility>

namespace sample {

Engine::Engine(std::string name) : name_(std::move(name)) {}

std::string Engine::run(int jobs) const {
    Worker worker(name_);
    return worker.process(jobs);
}

}  // namespace sample
