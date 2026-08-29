#include "sample/engine.hpp"

#include <iostream>

int main() {
    sample::Engine engine("demo");
    std::cout << engine.run(3) << '\n';
    return 0;
}
