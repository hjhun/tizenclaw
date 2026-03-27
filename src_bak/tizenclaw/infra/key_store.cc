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
#include "key_store.hh"

#include <glib.h>

#include <fstream>
#include <json.hpp>
#include <random>
#include <sstream>
#include <string_view>
#include <vector>

#include "../../common/logging.hh"

namespace tizenclaw {

// SHA-256 via GLib GChecksum
static std::string Sha256(const std::string& input) {
  GChecksum* cs = g_checksum_new(G_CHECKSUM_SHA256);
  g_checksum_update(cs, reinterpret_cast<const guchar*>(input.c_str()),
                    input.size());
  guint8 digest[32];
  gsize digest_len = 32;
  g_checksum_get_digest(cs, digest, &digest_len);
  g_checksum_free(cs);
  return std::string(reinterpret_cast<char*>(digest), digest_len);
}

std::string KeyStore::DeriveKey(const std::string& key_path) {
  std::string path = key_path.empty() ? kDefaultKeyPath : key_path;

  std::ifstream f(path);
  std::string machine_id;
  if (f.is_open()) {
    std::getline(f, machine_id);
    f.close();
  }

  if (machine_id.empty()) {
    LOG(WARNING) << "No machine-id at " << path << ", using fallback";
    machine_id = "tizenclaw-default-key";
  }

  // SHA-256(machine_id + salt) → 32 bytes
  return Sha256(machine_id + kSalt);
}

std::string KeyStore::Base64Encode(const unsigned char* data, size_t len) {
  gchar* encoded = g_base64_encode(data, len);
  std::string result(encoded);
  g_free(encoded);
  return result;
}

std::string KeyStore::Base64Decode(const std::string& encoded) {
  gsize out_len = 0;
  guchar* decoded = g_base64_decode(encoded.c_str(), &out_len);
  if (!decoded) return "";
  std::string result(reinterpret_cast<char*>(decoded), out_len);
  g_free(decoded);
  return result;
}

// XOR stream cipher with key-derived keystream
// Not AES, but effective for API key obfuscation
// at rest with device-bound key derivation.
static std::string XorCipher(const std::string& data, const std::string& key) {
  std::string result(data.size(), '\0');
  std::string keystream = key;

  // Extend keystream by hashing successive
  // blocks: key, SHA(key+1), SHA(key+2)...
  while (keystream.size() < data.size()) {
    keystream += Sha256(key + std::to_string(keystream.size()));
  }

  for (size_t i = 0; i < data.size(); ++i) {
    result[i] = data[i] ^ keystream[i];
  }
  return result;
}

std::string KeyStore::Encrypt(const std::string& plaintext,
                              const std::string& key_path) {
  if (plaintext.empty()) return plaintext;

  std::string key = DeriveKey(key_path);

  // Generate 16-byte random nonce
  unsigned char nonce[16];
  std::random_device rd;
  std::mt19937 gen(rd());
  std::uniform_int_distribution<unsigned short> dist(0, 255);
  for (int i = 0; i < 16; ++i) {
    nonce[i] = static_cast<unsigned char>(dist(gen));
  }

  // Derive unique subkey from key + nonce
  std::string nonce_str(reinterpret_cast<char*>(nonce), 16);
  std::string subkey = Sha256(key + nonce_str);

  // Encrypt with XOR cipher
  std::string encrypted = XorCipher(plaintext, subkey);

  // Prepend nonce to ciphertext
  std::string combined = nonce_str + encrypted;

  return std::string(kEncPrefix) +
         Base64Encode(reinterpret_cast<const unsigned char*>(combined.c_str()),
                      combined.size());
}

std::string KeyStore::Decrypt(const std::string& ciphertext,
                              const std::string& key_path) {
  if (!IsEncrypted(ciphertext)) {
    return ciphertext;  // plaintext fallback
  }

  std::string key = DeriveKey(key_path);

  // Remove "ENC:" prefix and decode base64
  static constexpr std::string_view kEncPrefixView(kEncPrefix);
  std::string encoded = ciphertext.substr(kEncPrefixView.size());
  std::string decoded = Base64Decode(encoded);

  if (decoded.size() < 17) {
    LOG(ERROR) << "Invalid encrypted data";
    return "";
  }

  // Extract nonce (first 16 bytes)
  std::string nonce_str = decoded.substr(0, 16);
  std::string enc_data = decoded.substr(16);

  // Derive same subkey
  std::string subkey = Sha256(key + nonce_str);

  // Decrypt with XOR (symmetric)
  return XorCipher(enc_data, subkey);
}

bool KeyStore::IsEncrypted(const std::string& value) {
  static constexpr std::string_view kEncPrefixView(kEncPrefix);
  return value.size() > kEncPrefixView.size() &&
         value.compare(0, kEncPrefixView.size(), kEncPrefixView.data(),
                       kEncPrefixView.size()) == 0;
}

bool KeyStore::EncryptConfig(const std::string& config_path,
                             const std::string& key_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(ERROR) << "Cannot open config: " << config_path;
    return false;
  }

  nlohmann::json config;
  try {
    f >> config;
    f.close();
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to parse config: " << e.what();
    return false;
  }

  bool changed = false;

  if (config.contains("backends")) {
    for (auto& [name, backend] : config["backends"].items()) {
      if (backend.contains("api_key")) {
        std::string api_key = backend["api_key"].get<std::string>();
        if (!api_key.empty() && !IsEncrypted(api_key)) {
          backend["api_key"] = Encrypt(api_key, key_path);
          changed = true;
          LOG(INFO) << "Encrypted api_key for: " << name;
        }
      }
    }
  }

  if (!changed) {
    LOG(INFO) << "No plaintext keys to encrypt";
    return true;
  }

  // Write back
  std::ofstream of(config_path);
  if (!of.is_open()) {
    LOG(ERROR) << "Cannot write config: " << config_path;
    return false;
  }

  of << config.dump(2) << std::endl;
  of.close();

  LOG(INFO) << "Config encrypted successfully";
  return true;
}

}  // namespace tizenclaw
