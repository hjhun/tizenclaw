#include <gtest/gtest.h>
#include <fstream>
#include <unistd.h>

#include "webhook_channel.hh"

using namespace tizenclaw;


TEST(WebhookChannelTest, VerifyHmacValid) {
    // Known test vector: HMAC-SHA256
    std::string secret = "test_secret_key";
    std::string payload = "hello world";

    // Compute expected HMAC
    GHmac* hmac = g_hmac_new(
        G_CHECKSUM_SHA256,
        reinterpret_cast<const guchar*>(
            secret.data()),
        secret.size());
    g_hmac_update(
        hmac,
        reinterpret_cast<const guchar*>(
            payload.data()),
        payload.size());
    std::string expected =
        std::string("sha256=") +
        g_hmac_get_string(hmac);
    g_hmac_unref(hmac);

    EXPECT_TRUE(WebhookChannel::VerifyHmac(
        secret, payload, expected));
}

TEST(WebhookChannelTest, VerifyHmacInvalid) {
    std::string secret = "test_secret_key";
    std::string payload = "hello world";
    std::string bad_sig = "sha256=deadbeef";

    EXPECT_FALSE(WebhookChannel::VerifyHmac(
        secret, payload, bad_sig));
}

TEST(WebhookChannelTest,
    VerifyHmacEmptySecret) {
    // Empty secret skips verification
    EXPECT_TRUE(WebhookChannel::VerifyHmac(
        "", "any payload", ""));
}

TEST(WebhookChannelTest,
    VerifyHmacEmptySignature) {
    // Non-empty secret but empty signature
    EXPECT_FALSE(WebhookChannel::VerifyHmac(
        "secret", "payload", ""));
}

TEST(WebhookChannelTest,
    VerifyHmacWithoutPrefix) {
    // Signature without sha256= prefix
    std::string secret = "key";
    std::string payload = "data";

    GHmac* hmac = g_hmac_new(
        G_CHECKSUM_SHA256,
        reinterpret_cast<const guchar*>(
            secret.data()),
        secret.size());
    g_hmac_update(
        hmac,
        reinterpret_cast<const guchar*>(
            payload.data()),
        payload.size());
    std::string hex = g_hmac_get_string(hmac);
    g_hmac_unref(hmac);

    // Without prefix should still match
    EXPECT_TRUE(WebhookChannel::VerifyHmac(
        secret, payload, hex));
}

TEST(WebhookChannelTest, StartWithoutConfig) {
    // Without config file, Start() should
    // return false (graceful failure)
    WebhookChannel wh(nullptr);
    EXPECT_FALSE(wh.Start());
    EXPECT_FALSE(wh.IsRunning());
}

TEST(WebhookChannelTest, GetName) {
    WebhookChannel wh(nullptr);
    EXPECT_EQ(wh.GetName(), "webhook");
}
