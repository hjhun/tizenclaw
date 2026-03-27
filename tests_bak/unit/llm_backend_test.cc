#include <gtest/gtest.h>
#include "llm_backend.hh"
#include "gemini_backend.hh"
#include "openai_backend.hh"
#include "anthropic_backend.hh"
#include "ollama_backend.hh"

using namespace tizenclaw;


// -----------------------------------------------
// Factory Tests
// -----------------------------------------------

class LlmBackendFactoryTest
    : public ::testing::Test {};

TEST_F(LlmBackendFactoryTest,
       CreateGeminiBackend) {
  auto b = LlmBackendFactory::Create("gemini");
  ASSERT_NE(b, nullptr);
  EXPECT_EQ(b->GetName(), "gemini");
}

TEST_F(LlmBackendFactoryTest,
       CreateOpenAiBackend) {
  auto b = LlmBackendFactory::Create("openai");
  ASSERT_NE(b, nullptr);
  EXPECT_EQ(b->GetName(), "openai");
}

TEST_F(LlmBackendFactoryTest,
       CreateChatGptAlias) {
  auto b = LlmBackendFactory::Create("chatgpt");
  ASSERT_NE(b, nullptr);
  EXPECT_EQ(b->GetName(), "openai");
}

TEST_F(LlmBackendFactoryTest,
       CreateXaiBackend) {
  auto b = LlmBackendFactory::Create("xai");
  ASSERT_NE(b, nullptr);
  // xAI uses OpenAiBackend
  EXPECT_EQ(b->GetName(), "openai");
}

TEST_F(LlmBackendFactoryTest,
       CreateGrokAlias) {
  auto b = LlmBackendFactory::Create("grok");
  ASSERT_NE(b, nullptr);
}

TEST_F(LlmBackendFactoryTest,
       CreateAnthropicBackend) {
  auto b =
      LlmBackendFactory::Create("anthropic");
  ASSERT_NE(b, nullptr);
  EXPECT_EQ(b->GetName(), "anthropic");
}

TEST_F(LlmBackendFactoryTest,
       CreateClaudeAlias) {
  auto b = LlmBackendFactory::Create("claude");
  ASSERT_NE(b, nullptr);
  EXPECT_EQ(b->GetName(), "anthropic");
}

TEST_F(LlmBackendFactoryTest,
       CreateOllamaBackend) {
  auto b = LlmBackendFactory::Create("ollama");
  ASSERT_NE(b, nullptr);
  EXPECT_EQ(b->GetName(), "ollama");
}

TEST_F(LlmBackendFactoryTest,
       CreateUnknownReturnsNull) {
  auto b = LlmBackendFactory::Create("unknown");
  EXPECT_EQ(b, nullptr);
}

TEST_F(LlmBackendFactoryTest,
       CreateEmptyReturnsNull) {
  auto b = LlmBackendFactory::Create("");
  EXPECT_EQ(b, nullptr);
}

// -----------------------------------------------
// GeminiBackend Init Tests
// -----------------------------------------------

class GeminiBackendTest
    : public ::testing::Test {
protected:
  GeminiBackend backend;
};

TEST_F(GeminiBackendTest,
       InitWithEmptyKeyFails) {
  nlohmann::json config = {
      {"api_key", ""},
      {"model", "gemini-2.5-flash"}
  };
  EXPECT_FALSE(backend.Initialize(config));
}

TEST_F(GeminiBackendTest,
       InitWithValidKeySucceeds) {
  nlohmann::json config = {
      {"api_key", "test_key_123"},
      {"model", "gemini-2.5-flash"}
  };
  EXPECT_TRUE(backend.Initialize(config));
}

TEST_F(GeminiBackendTest,
       InitUsesDefaultModel) {
  nlohmann::json config = {
      {"api_key", "test_key"}
  };
  EXPECT_TRUE(backend.Initialize(config));
  EXPECT_EQ(backend.GetName(), "gemini");
}

// -----------------------------------------------
// OpenAiBackend Init Tests
// -----------------------------------------------

class OpenAiBackendTest
    : public ::testing::Test {
protected:
  OpenAiBackend backend;
};

TEST_F(OpenAiBackendTest,
       InitWithEmptyKeyFails) {
  nlohmann::json config = {
      {"api_key", ""}
  };
  EXPECT_FALSE(backend.Initialize(config));
}

TEST_F(OpenAiBackendTest,
       InitWithValidKeySucceeds) {
  nlohmann::json config = {
      {"api_key", "sk-test-123"},
      {"model", "gpt-4o"}
  };
  EXPECT_TRUE(backend.Initialize(config));
}

TEST_F(OpenAiBackendTest,
       InitWithCustomEndpoint) {
  nlohmann::json config = {
      {"api_key", "sk-test"},
      {"endpoint", "https://custom.api/v1"},
      {"provider_name", "xai"}
  };
  EXPECT_TRUE(backend.Initialize(config));
  EXPECT_EQ(backend.GetName(), "xai");
}

// -----------------------------------------------
// AnthropicBackend Init Tests
// -----------------------------------------------

class AnthropicBackendTest
    : public ::testing::Test {
protected:
  AnthropicBackend backend;
};

TEST_F(AnthropicBackendTest,
       InitWithEmptyKeyFails) {
  nlohmann::json config = {
      {"api_key", ""}
  };
  EXPECT_FALSE(backend.Initialize(config));
}

TEST_F(AnthropicBackendTest,
       InitWithValidKeySucceeds) {
  nlohmann::json config = {
      {"api_key", "sk-ant-test"}
  };
  EXPECT_TRUE(backend.Initialize(config));
  EXPECT_EQ(backend.GetName(), "anthropic");
}

// -----------------------------------------------
// OllamaBackend Init Tests
// -----------------------------------------------

class OllamaBackendTest
    : public ::testing::Test {
protected:
  OllamaBackend backend;
};

TEST_F(OllamaBackendTest,
       InitAlwaysSucceeds) {
  // Ollama doesn't require API key
  nlohmann::json config = {
      {"model", "llama3"},
      {"endpoint", "http://localhost:11434"}
  };
  EXPECT_TRUE(backend.Initialize(config));
  EXPECT_EQ(backend.GetName(), "ollama");
}

TEST_F(OllamaBackendTest,
       InitWithDefaultConfig) {
  nlohmann::json config =
      nlohmann::json::object();
  config["model"] = "llama3";
  EXPECT_TRUE(backend.Initialize(config));
}

// -----------------------------------------------
// LlmResponse Helper Tests
// -----------------------------------------------

TEST(LlmResponseTest,
     HasToolCallsWhenEmpty) {
  LlmResponse resp;
  EXPECT_FALSE(resp.HasToolCalls());
}

TEST(LlmResponseTest,
     HasToolCallsWhenPresent) {
  LlmResponse resp;
  LlmToolCall tc;
  tc.name = "list_apps";
  tc.args = {};
  resp.tool_calls.push_back(tc);
  EXPECT_TRUE(resp.HasToolCalls());
}
