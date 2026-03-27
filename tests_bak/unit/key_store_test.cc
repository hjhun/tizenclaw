#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <unistd.h>
#include <json.hpp>
#include "key_store.hh"

using namespace tizenclaw;


class KeyStoreTest : public ::testing::Test {
protected:
    void SetUp() override {
        const char* test_name = ::testing::UnitTest::GetInstance()->current_test_info()->name();
        key_path_ = std::string("test_machine_id_") + test_name;
        config_path_ = std::string("test_encrypt_config_") + test_name + ".json";
        // Create a fake machine-id for testing
        std::ofstream f(key_path_);
        f << "test-machine-id-12345678" << std::endl;
        f.close();
    }

    void TearDown() override {
        unlink(key_path_.c_str());
        unlink(config_path_.c_str());
    }

    std::string key_path_;
    std::string config_path_;
};

TEST_F(KeyStoreTest, EncryptDecryptRoundtrip) {
    std::string plaintext = "sk-test-key-12345";
    std::string encrypted =
        KeyStore::Encrypt(plaintext, key_path_);

    // Encrypted value should start with ENC:
    EXPECT_TRUE(
        KeyStore::IsEncrypted(encrypted));

    // Decrypt back
    std::string decrypted =
        KeyStore::Decrypt(encrypted, key_path_);
    EXPECT_EQ(decrypted, plaintext);
}

TEST_F(KeyStoreTest, IsEncryptedDetection) {
    EXPECT_TRUE(KeyStore::IsEncrypted(
        "ENC:abc123=="));
    EXPECT_FALSE(KeyStore::IsEncrypted(
        "sk-plain-key"));
    EXPECT_FALSE(KeyStore::IsEncrypted(""));
    EXPECT_FALSE(KeyStore::IsEncrypted("ENC:"));
}

TEST_F(KeyStoreTest,
       PlaintextFallbackOnDecrypt) {
    // Non-encrypted value returned as-is
    std::string plaintext = "sk-plain-key";
    EXPECT_EQ(
        KeyStore::Decrypt(plaintext, key_path_),
        plaintext);
}

TEST_F(KeyStoreTest, EmptyStringNotEncrypted) {
    std::string empty_enc =
        KeyStore::Encrypt("", key_path_);
    EXPECT_TRUE(empty_enc.empty());
}

TEST_F(KeyStoreTest, EncryptConfigInPlace) {
    // Write a test config
    std::ofstream f(config_path_);
    f << R"({
      "active_backend": "gemini",
      "backends": {
        "gemini": {
          "api_key": "plain-key-123",
          "model": "gemini-2.5-flash"
        },
        "openai": {
          "api_key": "sk-openai-key",
          "model": "gpt-4o"
        },
        "ollama": {
          "model": "llama3"
        }
      }
    })" << std::endl;
    f.close();

    // Encrypt the config
    EXPECT_TRUE(KeyStore::EncryptConfig(
        config_path_, key_path_));

    // Read back and verify
    std::ifstream rf(config_path_);
    nlohmann::json config;
    rf >> config;
    rf.close();

    // gemini and openai keys should be encrypted
    std::string gemini_key =
        config["backends"]["gemini"]["api_key"]
            .get<std::string>();
    EXPECT_TRUE(
        KeyStore::IsEncrypted(gemini_key));

    std::string openai_key =
        config["backends"]["openai"]["api_key"]
            .get<std::string>();
    EXPECT_TRUE(
        KeyStore::IsEncrypted(openai_key));

    // ollama has no api_key — unaffected
    EXPECT_FALSE(
        config["backends"]["ollama"]
            .contains("api_key"));

    // Verify decrypt roundtrip
    EXPECT_EQ(
        KeyStore::Decrypt(gemini_key, key_path_),
        "plain-key-123");
    EXPECT_EQ(
        KeyStore::Decrypt(
            openai_key, key_path_),
        "sk-openai-key");
}

TEST_F(KeyStoreTest,
       DoubleEncryptionPrevented) {
    std::ofstream f(config_path_);
    f << R"({
      "backends": {
        "gemini": {
          "api_key": "test-key"
        }
      }
    })" << std::endl;
    f.close();

    // Encrypt once
    EXPECT_TRUE(KeyStore::EncryptConfig(
        config_path_, key_path_));

    // Read the encrypted key
    std::ifstream rf(config_path_);
    nlohmann::json config;
    rf >> config;
    rf.close();
    std::string first_enc =
        config["backends"]["gemini"]["api_key"]
            .get<std::string>();

    // Encrypt again — should not double-encrypt
    EXPECT_TRUE(KeyStore::EncryptConfig(
        config_path_, key_path_));

    std::ifstream rf2(
        config_path_);
    nlohmann::json config2;
    rf2 >> config2;
    rf2.close();
    std::string second_enc =
        config2["backends"]["gemini"]["api_key"]
            .get<std::string>();

    // Already encrypted, should remain same
    EXPECT_EQ(first_enc, second_enc);
}
