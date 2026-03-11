#include <chrono>
#include <iostream>
#include <filesystem>
#include <fstream>
#include <string>
#include "json.hpp" // assume available or omit for a simple read loop test

int main() {
    auto start = std::chrono::steady_clock::now();
    int count = 0;
    const std::string skills_dir = "/opt/usr/share/tizenclaw/tools/skills";
    for (int i=0; i<100; i++) { // simulate doing it 100 times to see if it's slow
        std::error_code ec;
        if (!std::filesystem::is_directory(skills_dir, ec)) continue;
        for (const auto& entry : std::filesystem::directory_iterator(skills_dir, ec)) {
            if (!entry.is_directory()) continue;
            auto dirname = entry.path().filename().string();
            if (dirname[0] == '.') continue;
            std::string manifest_path = entry.path() / "manifest.json";
            std::ifstream mf(manifest_path);
            if (!mf.is_open()) continue;
            std::string content((std::istreambuf_iterator<char>(mf)), std::istreambuf_iterator<char>());
            count++;
        }
    }
    auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
          std::chrono::steady_clock::now() - start).count();
    std::cout << "Read 100 times: " << elapsed << " ms, " << count << " files" << std::endl;
    return 0;
}
