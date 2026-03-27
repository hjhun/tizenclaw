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
#include "wordpiece_tokenizer.hh"

#include <algorithm>
#include <cctype>
#include <fstream>
#include <sstream>

#include "../../common/logging.hh"

namespace tizenclaw {

bool WordPieceTokenizer::LoadVocab(const std::string& vocab_path) {
  std::ifstream f(vocab_path);
  if (!f.is_open()) {
    LOG(ERROR) << "Cannot open vocab: " << vocab_path;
    return false;
  }

  vocab_.clear();
  std::string line;
  int64_t id = 0;
  while (std::getline(f, line)) {
    // Remove trailing whitespace
    while (!line.empty() && (line.back() == '\r' || line.back() == '\n' ||
                             line.back() == ' ')) {
      line.pop_back();
    }
    vocab_[line] = id++;
  }

  // Cache special token IDs
  auto find_id = [this](const char* tok) -> int64_t {
    auto it = vocab_.find(tok);
    return it != vocab_.end() ? it->second : 0;
  };

  cls_id_ = find_id("[CLS]");
  sep_id_ = find_id("[SEP]");
  unk_id_ = find_id("[UNK]");
  pad_id_ = find_id("[PAD]");

  LOG(INFO) << "Vocab loaded: " << vocab_.size() << " tokens"
            << " (CLS=" << cls_id_ << " SEP=" << sep_id_
            << " UNK=" << unk_id_ << ")";
  return !vocab_.empty();
}

std::string WordPieceTokenizer::NormalizeText(const std::string& text) {
  std::string result;
  result.reserve(text.size());

  for (unsigned char c : text) {
    // Convert to lowercase
    if (c >= 'A' && c <= 'Z') {
      result += static_cast<char>(c + 32);
    } else if (c >= 0x80) {
      // Keep non-ASCII as-is (basic handling)
      result += static_cast<char>(c);
    } else {
      result += static_cast<char>(c);
    }
  }
  return result;
}

std::vector<std::string> WordPieceTokenizer::BasicTokenize(
    const std::string& text) {
  std::vector<std::string> tokens;
  std::string current;

  for (size_t i = 0; i < text.size(); ++i) {
    unsigned char c = text[i];

    // Check if punctuation or whitespace
    bool is_punct = (c < 0x80) && std::ispunct(c);
    bool is_space = (c < 0x80) && std::isspace(c);

    if (is_space) {
      if (!current.empty()) {
        tokens.push_back(current);
        current.clear();
      }
    } else if (is_punct) {
      if (!current.empty()) {
        tokens.push_back(current);
        current.clear();
      }
      tokens.push_back(std::string(1, static_cast<char>(c)));
    } else {
      current += static_cast<char>(c);
    }
  }

  if (!current.empty()) {
    tokens.push_back(current);
  }

  return tokens;
}

std::vector<std::string> WordPieceTokenizer::WordPieceTokenize(
    const std::string& token) const {
  std::vector<std::string> sub_tokens;

  if (token.empty()) return sub_tokens;

  // Check if the whole token is in vocab
  if (vocab_.count(token)) {
    sub_tokens.push_back(token);
    return sub_tokens;
  }

  // WordPiece: try to split into sub-words
  size_t start = 0;
  while (start < token.size()) {
    size_t end = token.size();
    std::string best_match;
    bool found = false;

    while (start < end) {
      std::string substr = token.substr(start, end - start);
      if (start > 0) {
        substr = "##" + substr;
      }

      if (vocab_.count(substr)) {
        best_match = substr;
        found = true;
        break;
      }
      --end;
    }

    if (!found) {
      // Unknown character — use [UNK]
      sub_tokens.push_back("[UNK]");
      break;
    }

    sub_tokens.push_back(best_match);
    start = end;
  }

  return sub_tokens;
}

int64_t WordPieceTokenizer::TokenToId(const std::string& token) const {
  auto it = vocab_.find(token);
  return it != vocab_.end() ? it->second : unk_id_;
}

WordPieceTokenizer::TokenizedInput WordPieceTokenizer::Tokenize(
    const std::string& text, int max_length) const {
  TokenizedInput result;

  // Normalize
  std::string normalized = NormalizeText(text);

  // Basic tokenization
  auto basic_tokens = BasicTokenize(normalized);

  // WordPiece tokenization
  std::vector<std::string> wp_tokens;
  for (const auto& token : basic_tokens) {
    auto sub = WordPieceTokenize(token);
    for (auto& s : sub) {
      wp_tokens.push_back(std::move(s));
    }
  }

  // Truncate to max_length - 2 (for [CLS] and [SEP])
  int max_tokens = max_length - 2;
  if (static_cast<int>(wp_tokens.size()) > max_tokens) {
    wp_tokens.resize(max_tokens);
  }

  // Build input_ids: [CLS] + tokens + [SEP]
  result.input_ids.push_back(cls_id_);
  for (const auto& tok : wp_tokens) {
    result.input_ids.push_back(TokenToId(tok));
  }
  result.input_ids.push_back(sep_id_);

  // Attention mask: 1 for real tokens
  int seq_len = static_cast<int>(result.input_ids.size());
  result.attention_mask.resize(seq_len, 1);

  // Token type IDs: all 0 (single sequence)
  result.token_type_ids.resize(seq_len, 0);

  // Pad to max_length
  while (static_cast<int>(result.input_ids.size()) < max_length) {
    result.input_ids.push_back(pad_id_);
    result.attention_mask.push_back(0);
    result.token_type_ids.push_back(0);
  }

  return result;
}

}  // namespace tizenclaw
