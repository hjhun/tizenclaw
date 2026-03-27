//! WordPiece tokenizer for BERT-compatible models (all-MiniLM-L6-v2).
//!
//! Loads vocab.txt and tokenizes text into input_ids/attention_mask/token_type_ids
//! suitable for ONNX Runtime inference.

use std::collections::HashMap;
use std::io::BufRead;

/// Tokenized input ready for the ONNX model.
pub struct TokenizedInput {
    pub input_ids: Vec<i64>,
    pub attention_mask: Vec<i64>,
    pub token_type_ids: Vec<i64>,
}

/// BERT-compatible WordPiece tokenizer.
pub struct WordPieceTokenizer {
    vocab: HashMap<String, i64>,
    cls_id: i64,
    sep_id: i64,
    unk_id: i64,
    pad_id: i64,
}

impl WordPieceTokenizer {
    pub fn new() -> Self {
        Self {
            vocab: HashMap::new(),
            cls_id: 0,
            sep_id: 0,
            unk_id: 0,
            pad_id: 0,
        }
    }

    /// Load vocabulary from vocab.txt file.
    pub fn load_vocab(&mut self, vocab_path: &str) -> bool {
        let file = match std::fs::File::open(vocab_path) {
            Ok(f) => f,
            Err(e) => {
                log::error!("Cannot open vocab: {}: {}", vocab_path, e);
                return false;
            }
        };

        self.vocab.clear();
        let reader = std::io::BufReader::new(file);
        let mut id: i64 = 0;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            let trimmed = line.trim_end().to_string();
            self.vocab.insert(trimmed, id);
            id += 1;
        }

        let find_id = |vocab: &HashMap<String, i64>, tok: &str| -> i64 {
            vocab.get(tok).copied().unwrap_or(0)
        };

        self.cls_id = find_id(&self.vocab, "[CLS]");
        self.sep_id = find_id(&self.vocab, "[SEP]");
        self.unk_id = find_id(&self.vocab, "[UNK]");
        self.pad_id = find_id(&self.vocab, "[PAD]");

        log::info!(
            "Vocab loaded: {} tokens (CLS={} SEP={} UNK={})",
            self.vocab.len(),
            self.cls_id,
            self.sep_id,
            self.unk_id
        );
        !self.vocab.is_empty()
    }

    pub fn is_loaded(&self) -> bool {
        !self.vocab.is_empty()
    }

    /// Tokenize text into model input format.
    /// `max_length` includes [CLS] and [SEP] tokens.
    pub fn tokenize(&self, text: &str, max_length: usize) -> TokenizedInput {
        // Normalize
        let normalized = Self::normalize_text(text);

        // Basic tokenization
        let basic_tokens = Self::basic_tokenize(&normalized);

        // WordPiece tokenization
        let mut wp_tokens = Vec::new();
        for token in &basic_tokens {
            let sub = self.wordpiece_tokenize(token);
            wp_tokens.extend(sub);
        }

        // Truncate to max_length - 2 (for [CLS] and [SEP])
        let max_tokens = max_length.saturating_sub(2);
        if wp_tokens.len() > max_tokens {
            wp_tokens.truncate(max_tokens);
        }

        // Build input_ids: [CLS] + tokens + [SEP]
        let mut input_ids = Vec::with_capacity(max_length);
        input_ids.push(self.cls_id);
        for tok in &wp_tokens {
            input_ids.push(self.token_to_id(tok));
        }
        input_ids.push(self.sep_id);

        let seq_len = input_ids.len();
        let mut attention_mask = vec![1i64; seq_len];
        let mut token_type_ids = vec![0i64; seq_len];

        // Pad to max_length
        while input_ids.len() < max_length {
            input_ids.push(self.pad_id);
            attention_mask.push(0);
            token_type_ids.push(0);
        }

        TokenizedInput {
            input_ids,
            attention_mask,
            token_type_ids,
        }
    }

    // ─── Private helpers ────────────────────────

    fn normalize_text(text: &str) -> String {
        text.chars()
            .map(|c| {
                if c.is_ascii_uppercase() {
                    c.to_ascii_lowercase()
                } else {
                    c
                }
            })
            .collect()
    }

    fn basic_tokenize(text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();

        for c in text.chars() {
            if c.is_ascii_whitespace() {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            } else if c.is_ascii_punctuation() {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                tokens.push(c.to_string());
            } else {
                current.push(c);
            }
        }

        if !current.is_empty() {
            tokens.push(current);
        }
        tokens
    }

    fn wordpiece_tokenize(&self, token: &str) -> Vec<String> {
        if token.is_empty() {
            return Vec::new();
        }

        // Check if whole token is in vocab
        if self.vocab.contains_key(token) {
            return vec![token.to_string()];
        }

        let mut sub_tokens = Vec::new();
        let chars: Vec<char> = token.chars().collect();
        let mut start = 0;

        while start < chars.len() {
            let mut end = chars.len();
            let mut found = false;

            while start < end {
                let substr: String = chars[start..end].iter().collect();
                let candidate = if start > 0 {
                    format!("##{}", substr)
                } else {
                    substr
                };

                if self.vocab.contains_key(&candidate) {
                    sub_tokens.push(candidate);
                    found = true;
                    start = end;
                    break;
                }
                end -= 1;
            }

            if !found {
                sub_tokens.push("[UNK]".to_string());
                break;
            }
        }

        sub_tokens
    }

    fn token_to_id(&self, token: &str) -> i64 {
        self.vocab.get(token).copied().unwrap_or(self.unk_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tokenizer_with_vocab() -> WordPieceTokenizer {
        let mut t = WordPieceTokenizer::new();
        // Minimal BERT vocab
        let vocab_entries = vec![
            "[PAD]", "[UNK]", "[CLS]", "[SEP]", "[MASK]",
            "hello", "world", "the", "a", "is",
            "##ing", "##ed", "##ly", "test", "rust",
            ".", ",", "!", "good", "morning",
        ];
        for (i, tok) in vocab_entries.iter().enumerate() {
            t.vocab.insert(tok.to_string(), i as i64);
        }
        t.cls_id = 2;
        t.sep_id = 3;
        t.unk_id = 1;
        t.pad_id = 0;
        t
    }

    #[test]
    fn test_normalize_text_lowercase() {
        assert_eq!(
            WordPieceTokenizer::normalize_text("Hello WORLD"),
            "hello world"
        );
    }

    #[test]
    fn test_normalize_text_preserves_nonascii() {
        let input = "안녕하세요 ABC";
        let result = WordPieceTokenizer::normalize_text(input);
        assert!(result.contains("안녕하세요"));
        assert!(result.contains("abc"));
    }

    #[test]
    fn test_basic_tokenize_whitespace() {
        let tokens = WordPieceTokenizer::basic_tokenize("hello world");
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn test_basic_tokenize_punctuation() {
        let tokens = WordPieceTokenizer::basic_tokenize("hello, world!");
        assert_eq!(tokens, vec!["hello", ",", "world", "!"]);
    }

    #[test]
    fn test_basic_tokenize_empty() {
        let tokens = WordPieceTokenizer::basic_tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_output_structure() {
        let t = make_tokenizer_with_vocab();
        let result = t.tokenize("hello world", 10);

        // Should have max_length elements
        assert_eq!(result.input_ids.len(), 10);
        assert_eq!(result.attention_mask.len(), 10);
        assert_eq!(result.token_type_ids.len(), 10);

        // First token is [CLS]=2, then "hello"=5, "world"=6, [SEP]=3
        assert_eq!(result.input_ids[0], 2); // [CLS]
        assert_eq!(result.input_ids[1], 5); // hello
        assert_eq!(result.input_ids[2], 6); // world
        assert_eq!(result.input_ids[3], 3); // [SEP]

        // Attention mask: 1 for real tokens, 0 for padding
        assert_eq!(result.attention_mask[0], 1);
        assert_eq!(result.attention_mask[3], 1);
        assert_eq!(result.attention_mask[4], 0);

        // Token type IDs all 0
        assert!(result.token_type_ids.iter().all(|&v| v == 0));
    }

    #[test]
    fn test_tokenize_padding() {
        let t = make_tokenizer_with_vocab();
        let result = t.tokenize("hello", 8);

        // [CLS] hello [SEP] + 5 pads = 8
        assert_eq!(result.input_ids.len(), 8);
        assert_eq!(result.input_ids[0], 2);  // [CLS]
        assert_eq!(result.input_ids[2], 3);  // [SEP]
        // Remaining should be [PAD]=0
        for i in 3..8 {
            assert_eq!(result.input_ids[i], 0);
            assert_eq!(result.attention_mask[i], 0);
        }
    }

    #[test]
    fn test_tokenize_truncation() {
        let t = make_tokenizer_with_vocab();
        // max_length=4 means max 2 actual tokens (4 - [CLS] - [SEP])
        let result = t.tokenize("hello world test rust good", 4);
        assert_eq!(result.input_ids.len(), 4);
        assert_eq!(result.input_ids[0], 2); // [CLS]
        assert_eq!(result.input_ids[3], 3); // [SEP]
    }

    #[test]
    fn test_tokenize_unknown_word() {
        let t = make_tokenizer_with_vocab();
        // "unknown_xyz" is not in vocab at all
        let result = t.tokenize("unknown_xyz", 5);
        // Should use [UNK]=1
        assert!(result.input_ids.contains(&1));
    }

    #[test]
    fn test_new_tokenizer_not_loaded() {
        let t = WordPieceTokenizer::new();
        assert!(!t.is_loaded());
    }

    #[test]
    fn test_tokenizer_with_vocab_is_loaded() {
        let t = make_tokenizer_with_vocab();
        assert!(t.is_loaded());
    }
}
