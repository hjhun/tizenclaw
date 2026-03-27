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
#ifndef WORDPIECE_TOKENIZER_HH
#define WORDPIECE_TOKENIZER_HH

#include <cstdint>
#include <string>
#include <unordered_map>
#include <vector>

namespace tizenclaw {

// BERT-compatible WordPiece tokenizer for on-device
// embedding inference (all-MiniLM-L6-v2).
// Loads vocab.txt and tokenizes text into token IDs.
class WordPieceTokenizer {
 public:
  struct TokenizedInput {
    std::vector<int64_t> input_ids;
    std::vector<int64_t> attention_mask;
    std::vector<int64_t> token_type_ids;
  };

  // Load vocabulary from vocab.txt file
  [[nodiscard]] bool LoadVocab(const std::string& vocab_path);

  // Tokenize text into model input format
  // max_length includes [CLS] and [SEP] tokens
  [[nodiscard]] TokenizedInput Tokenize(const std::string& text,
                                        int max_length = 128) const;

  bool IsLoaded() const { return !vocab_.empty(); }

 private:
  // Lowercase and strip accents
  static std::string NormalizeText(const std::string& text);

  // Split text into initial tokens (whitespace + punctuation)
  static std::vector<std::string> BasicTokenize(const std::string& text);

  // WordPiece sub-word tokenization
  std::vector<std::string> WordPieceTokenize(const std::string& token) const;

  // Token to ID lookup
  int64_t TokenToId(const std::string& token) const;

  std::unordered_map<std::string, int64_t> vocab_;
  int64_t cls_id_ = 0;
  int64_t sep_id_ = 0;
  int64_t unk_id_ = 0;
  int64_t pad_id_ = 0;
};

}  // namespace tizenclaw

#endif  // WORDPIECE_TOKENIZER_HH
