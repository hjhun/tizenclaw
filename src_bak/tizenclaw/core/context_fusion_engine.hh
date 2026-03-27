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
#ifndef CONTEXT_FUSION_ENGINE_HH
#define CONTEXT_FUSION_ENGINE_HH

#include <json.hpp>
#include <string>
#include <vector>

#include "device_profiler.hh"

namespace tizenclaw {

// Situation severity level
enum class SituationLevel {
  kNormal = 0,
  kAdvisory = 1,   // informational, inject only
  kWarning = 2,    // warn user via channel
  kCritical = 3    // immediate action needed
};

// Combined assessment of device situation
struct SituationAssessment {
  SituationLevel level = SituationLevel::kNormal;
  double risk_score = 0.0;     // 0.0~1.0
  std::string summary;
  std::vector<std::string> factors;
  std::vector<std::string> suggestions;
};

// Fuses multiple independent signals (battery,
// memory, network, app state) into a unified
// risk score and situation assessment.
class ContextFusionEngine {
 public:
  ContextFusionEngine() = default;
  ~ContextFusionEngine() = default;

  // Fuse profile snapshot and device state
  // into a situation assessment
  [[nodiscard]] SituationAssessment Fuse(
      const ProfileSnapshot& profile,
      const nlohmann::json& device_state) const;

  // Convert SituationLevel to string
  [[nodiscard]] static std::string LevelToString(
      SituationLevel level);

  // Convert assessment to JSON
  [[nodiscard]] static nlohmann::json ToJson(
      const SituationAssessment& assessment);

 private:
  // Individual risk evaluators
  [[nodiscard]] double EvalBatteryRisk(
      const ProfileSnapshot& p) const;
  [[nodiscard]] double EvalMemoryRisk(
      const ProfileSnapshot& p) const;
  [[nodiscard]] double EvalNetworkRisk(
      const ProfileSnapshot& p) const;
  [[nodiscard]] double EvalAnomalyRisk(
      const ProfileSnapshot& p) const;

  // Determine overall level from risk score
  [[nodiscard]] static SituationLevel
  ScoreToLevel(double score);

  // Risk weight configuration
  static constexpr double kBatteryWeight = 0.35;
  static constexpr double kMemoryWeight = 0.30;
  static constexpr double kNetworkWeight = 0.20;
  static constexpr double kAnomalyWeight = 0.15;
};

}  // namespace tizenclaw

#endif  // CONTEXT_FUSION_ENGINE_HH
