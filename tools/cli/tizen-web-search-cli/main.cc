/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd All Rights Reserved
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "search_controller.hh"

#include <iostream>
#include <string>

namespace {

constexpr const char kUsage[] = R"(Usage:
  tizen-web-search-cli --query <QUERY> [--engine <ENGINE>]

Engines:
  naver (default), google, brave,
  gemini, grok, kimi, perplexity
)";

void PrintUsage() {
  std::cerr << kUsage;
}

}  // namespace

int main(int argc, char* argv[]) {
  std::string query;
  std::string engine;

  for (int i = 1; i < argc - 1; ++i) {
    std::string arg = argv[i];
    if (arg == "--query")
      query = argv[i + 1];
    else if (arg == "--engine")
      engine = argv[i + 1];
  }

  if (query.empty()) {
    PrintUsage();
    return 1;
  }

  tizenclaw::cli::SearchController c;
  std::cout << c.Search(query, engine) << std::endl;

  return 0;
}
