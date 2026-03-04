#include <gtest/gtest.h>
#include "container_engine.h"

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
    EXPECT_TRUE(engine->Initialize());
}

TEST_F(ContainerEngineTest, StartWithoutInit) {
    // Should fail gracefully because engine is not initialized
    EXPECT_FALSE(engine->StartContainer("test_container", "/tmp"));
}

TEST_F(ContainerEngineTest, BasicStartStop) {
    ASSERT_TRUE(engine->Initialize());

    // Creating a container struct should theoretically succeed 
    // Even if it fails to actually 'start' the process due to missing config,
    // StartContainer is designed to gracefully execute and return true in Phase 2 mock mode.
    EXPECT_TRUE(engine->StartContainer("unit_test_container", "/tmp"));
    
    // Stop should also succeed
    EXPECT_TRUE(engine->StopContainer("unit_test_container"));
}
