#include <gtest/gtest.h>
#include "http_client.hh"

using namespace tizenclaw;


class HttpClientTest : public ::testing::Test {};

TEST_F(HttpClientTest,
       PostToInvalidUrlFails) {
  auto resp = HttpClient::Post(
      "http://invalid.nonexistent.host.test/",
      {{"Content-Type", "application/json"}},
      "{}",
      1,    // single attempt
      2,    // short connect timeout
      5);   // short request timeout

  EXPECT_FALSE(resp.success);
  EXPECT_FALSE(resp.error.empty());
}

TEST_F(HttpClientTest,
       PostWithEmptyUrlFails) {
  auto resp = HttpClient::Post(
      "",
      {},
      "{}",
      1, 2, 5);

  EXPECT_FALSE(resp.success);
}

TEST_F(HttpClientTest,
       ResponseFieldsInitialized) {
  HttpResponse resp;
  EXPECT_EQ(resp.status_code, 0);
  EXPECT_TRUE(resp.body.empty());
  EXPECT_FALSE(resp.success);
  EXPECT_TRUE(resp.error.empty());
}

TEST_F(HttpClientTest,
       GetToInvalidUrlFails) {
  auto resp = HttpClient::Get(
      "http://invalid.nonexistent.host.test/",
      {},
      1,    // single attempt
      2,    // short connect timeout
      5);   // short request timeout

  EXPECT_FALSE(resp.success);
  EXPECT_FALSE(resp.error.empty());
}

TEST_F(HttpClientTest,
       GetWithEmptyUrlFails) {
  auto resp = HttpClient::Get(
      "",
      {},
      1, 2, 5);

  EXPECT_FALSE(resp.success);
}
