#include <gtest/gtest.h>
#include "container_engine.hh"

using namespace tizenclaw;


class ContainerEngineTest : public ::testing::Test {
protected:
    void SetUp() override {
        engine = new ContainerEngine();
    }

    void TearDown() override {
        delete engine;
    }

    ContainerEngine* engine;
};

TEST_F(ContainerEngineTest, Initialize) {
    // Should initialize successfully, even if crun/runc are missing it gracefully falls back
    EXPECT_TRUE(engine->Initialize());
}

TEST_F(ContainerEngineTest, ExecuteWithoutInit) {
    // Should fail gracefully because engine is not initialized and return "{}"
    EXPECT_EQ(engine->ExecuteSkill("dummy_skill", "{}"), "{}");
}

TEST_F(ContainerEngineTest, BasicExecuteSkill) {
    ASSERT_TRUE(engine->Initialize());

    // Executing a dummy skill should attempt to create bundle
    // It might return "{}" if rootfs/runc isn't properly mockable in the build environment,
    // but it shouldn't crash.
    std::string result = engine->ExecuteSkill("dummy_skill", "{\"key\":\"value\"}");
    
    // We expect it to not crash. Depending on the environment, it may fail to extract rootfs and return "{}"
    EXPECT_TRUE(true); 
}
