#include <gtest/gtest.h>
#include <fstream>
#include <unistd.h>
#include "user_profile_store.hh"

using namespace tizenclaw;

class UserProfileStoreTest : public ::testing::Test {
 protected:
  void SetUp() override {
    store_ = new UserProfileStore();
    db_path_ = std::string("test_profiles_") +
        ::testing::UnitTest::GetInstance()
            ->current_test_info()->name() + ".json";
  }

  void TearDown() override {
    delete store_;
    unlink(db_path_.c_str());
  }

  UserProfileStore* store_;
  std::string db_path_;
};

TEST_F(UserProfileStoreTest, InitializeEmpty) {
  EXPECT_TRUE(store_->Initialize(db_path_));
  
  // By default, unknown session yields empty user id 
  // which GetProfile maps to a generic guest
  auto profile = store_->GetProfile("");
  EXPECT_EQ(profile.role, UserRole::kGuest);
  EXPECT_EQ(profile.name, "Guest");
}

TEST_F(UserProfileStoreTest, LoadExistingSuccess) {
  std::ofstream f(db_path_);
  f << R"({
    "profiles": [
      {
        "user_id": "dad",
        "name": "Dad",
        "role": "admin",
        "voice_id": "v123"
      },
      {
        "user_id": "kid",
        "name": "Tommy",
        "role": "child"
      }
    ]
  })" << std::endl;
  f.close();

  EXPECT_TRUE(store_->Initialize(db_path_));

  auto dad = store_->GetProfile("dad");
  EXPECT_EQ(dad.role, UserRole::kAdmin);
  EXPECT_EQ(dad.name, "Dad");
  EXPECT_EQ(dad.voice_id, "v123");

  auto kid = store_->GetProfile("kid");
  EXPECT_EQ(kid.role, UserRole::kChild);
}

TEST_F(UserProfileStoreTest, UpsertAndPersist) {
  ASSERT_TRUE(store_->Initialize(db_path_));

  UserProfile p;
  p.user_id = "mom";
  p.name = "Mom";
  p.role = UserRole::kAdmin;
  p.preferences = {{"theme", "dark"}};

  EXPECT_TRUE(store_->UpsertProfile(p));

  // Reload into a new store to verify persistence
  UserProfileStore new_store;
  EXPECT_TRUE(new_store.Initialize(db_path_));

  auto loaded = new_store.GetProfile("mom");
  EXPECT_EQ(loaded.name, "Mom");
  EXPECT_EQ(loaded.role, UserRole::kAdmin);
  EXPECT_EQ(loaded.preferences["theme"], "dark");
}

TEST_F(UserProfileStoreTest, DeleteProfile) {
  ASSERT_TRUE(store_->Initialize(db_path_));

  UserProfile p;
  p.user_id = "test_user";
  p.name = "Test";
  store_->UpsertProfile(p);

  EXPECT_EQ(store_->GetProfile("test_user").name, "Test");
  
  EXPECT_TRUE(store_->DeleteProfile("test_user"));
  
  auto deleted = store_->GetProfile("test_user");
  EXPECT_EQ(deleted.role, UserRole::kGuest); // Defaults to guest
}

TEST_F(UserProfileStoreTest, SessionBinding) {
  ASSERT_TRUE(store_->Initialize(db_path_));

  UserProfile p;
  p.user_id = "test_user";
  store_->UpsertProfile(p);

  store_->BindSession("session_xyz", "test_user");
  
  EXPECT_EQ(store_->GetUserIdForSession("session_xyz"), "test_user");
  
  auto profile = store_->GetProfile(store_->GetUserIdForSession("session_xyz"));
  EXPECT_EQ(profile.user_id, "test_user");
}

TEST_F(UserProfileStoreTest, StringToRoleConversions) {
  EXPECT_EQ(UserProfileStore::StringToRole("admin"), UserRole::kAdmin);
  EXPECT_EQ(UserProfileStore::StringToRole("member"), UserRole::kMember);
  EXPECT_EQ(UserProfileStore::StringToRole("child"), UserRole::kChild);
  EXPECT_EQ(UserProfileStore::StringToRole("guest"), UserRole::kGuest);
  EXPECT_EQ(UserProfileStore::StringToRole("invalid"), UserRole::kGuest);

  EXPECT_EQ(UserProfileStore::RoleToString(UserRole::kAdmin), "admin");
  EXPECT_EQ(UserProfileStore::RoleToString(UserRole::kMember), "member");
  EXPECT_EQ(UserProfileStore::RoleToString(UserRole::kChild), "child");
  EXPECT_EQ(UserProfileStore::RoleToString(UserRole::kGuest), "guest");
}
