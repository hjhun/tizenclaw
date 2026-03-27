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
#ifndef KEY_STORE_HH
#define KEY_STORE_HH

#include <string>

namespace tizenclaw {

class KeyStore {
 public:
  // Encrypt plaintext → "ENC:" + base64
  static std::string Encrypt(const std::string& plaintext,
                             const std::string& key_path = "");

  // Decrypt "ENC:xxx" → plaintext
  static std::string Decrypt(const std::string& ciphertext,
                             const std::string& key_path = "");

  // Check if value starts with "ENC:"
  static bool IsEncrypted(const std::string& value);

  // Encrypt all api_key fields in config
  static bool EncryptConfig(const std::string& config_path,
                            const std::string& key_path = "");

 private:
  // Derive 32-byte key from machine-id
  // Uses GLib SHA-256 (no openssl needed)
  static std::string DeriveKey(const std::string& key_path);

  // Base64 encode/decode (GLib)
  static std::string Base64Encode(const unsigned char* data, size_t len);
  static std::string Base64Decode(const std::string& encoded);

  static constexpr const char* kEncPrefix = "ENC:";
  static constexpr const char* kDefaultKeyPath = "/etc/machine-id";
  static constexpr const char* kSalt = "TizenClaw_KeyStore_v1";
};

}  // namespace tizenclaw

#endif  // KEY_STORE_HH
