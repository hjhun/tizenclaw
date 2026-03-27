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
#include "context_fusion_engine.hh"

#include <algorithm>
#include <cmath>

#include "../../common/logging.hh"

namespace tizenclaw {

SituationAssessment ContextFusionEngine::Fuse(
    const ProfileSnapshot& profile,
    const nlohmann::json& device_state) const {
  SituationAssessment result;

  // Evaluate individual risk dimensions
  double bat_risk = EvalBatteryRisk(profile);
  double mem_risk = EvalMemoryRisk(profile);
  double net_risk = EvalNetworkRisk(profile);
  double ano_risk = EvalAnomalyRisk(profile);

  // Weighted fusion
  result.risk_score =
      kBatteryWeight * bat_risk +
      kMemoryWeight * mem_risk +
      kNetworkWeight * net_risk +
      kAnomalyWeight * ano_risk;

  // Clamp to [0, 1]
  result.risk_score =
      std::clamp(result.risk_score, 0.0, 1.0);

  result.level = ScoreToLevel(result.risk_score);

  // Collect risk factors and suggestions

  // Battery factors
  if (bat_risk > 0.3) {
    if (profile.battery_level >= 0 &&
        profile.battery_level < 15) {
      result.factors.push_back(
          "Battery critically low (" +
          std::to_string(profile.battery_level) +
          "%)");
      result.suggestions.push_back(
          "Connect to charger immediately");
    } else if (profile.battery_level < 30) {
      result.factors.push_back(
          "Battery low (" +
          std::to_string(profile.battery_level) +
          "%)");
      result.suggestions.push_back(
          "Consider charging soon");
    }
    if (profile.battery_drain_rate > 1.0) {
      result.factors.push_back(
          "High battery drain rate (" +
          std::to_string(
              (int)(profile.battery_drain_rate *
                    100) /
              100.0) +
          "%/min)");
      result.suggestions.push_back(
          "Close resource-intensive apps");
    }
  }

  // Memory factors
  if (mem_risk > 0.3) {
    if (profile.memory_warning_count > 0) {
      result.factors.push_back(
          "Memory pressure (" +
          std::to_string(
              profile.memory_warning_count) +
          " warnings in 30min)");
      result.suggestions.push_back(
          "Close unused applications to free "
          "memory");
    }
    if (profile.memory_trend == "critical") {
      result.factors.push_back(
          "Possible memory leak detected");
      result.suggestions.push_back(
          "Restart device if memory pressure "
          "persists");
    }
  }

  // Network factors
  if (net_risk > 0.3) {
    if (profile.network_drop_count > 0) {
      result.factors.push_back(
          "Network instability (" +
          std::to_string(
              profile.network_drop_count) +
          " drops in 30min)");
      result.suggestions.push_back(
          "Check WiFi signal or mobile data "
          "connection");
    }
    if (profile.network_status ==
        "disconnected") {
      result.factors.push_back(
          "Network disconnected");
      result.suggestions.push_back(
          "Reconnect to WiFi or enable "
          "mobile data");
    }
  }

  // Anomaly factors
  if (!profile.anomalies.empty()) {
    for (const auto& a : profile.anomalies) {
      std::string detail =
          a.value("detail", "unknown anomaly");
      result.factors.push_back(detail);
    }
  }

  // Build summary
  if (result.factors.empty()) {
    result.summary =
        "Device is operating normally";
  } else {
    result.summary =
        std::to_string(result.factors.size()) +
        " risk factor(s) detected, "
        "overall risk: " +
        LevelToString(result.level);
  }

  return result;
}

double ContextFusionEngine::EvalBatteryRisk(
    const ProfileSnapshot& p) const {
  if (p.charging) return 0.0;
  if (p.battery_level < 0) return 0.0;

  double risk = 0.0;

  // Level-based risk
  if (p.battery_level < 5) {
    risk = 1.0;
  } else if (p.battery_level < 15) {
    risk = 0.8;
  } else if (p.battery_level < 30) {
    risk = 0.4;
  } else if (p.battery_level < 50) {
    risk = 0.1;
  }

  // Drain rate amplifier
  if (p.battery_drain_rate > 2.0) {
    risk = std::min(1.0, risk + 0.3);
  } else if (p.battery_drain_rate > 1.0) {
    risk = std::min(1.0, risk + 0.15);
  }

  return risk;
}

double ContextFusionEngine::EvalMemoryRisk(
    const ProfileSnapshot& p) const {
  double risk = 0.0;

  if (p.memory_trend == "critical") {
    risk = 0.9;
  } else if (p.memory_trend == "rising") {
    risk = 0.5;
  }

  // Warning count amplifier
  if (p.memory_warning_count >= 3) {
    risk = std::min(1.0, risk + 0.3);
  } else if (p.memory_warning_count >= 1) {
    risk = std::min(1.0, risk + 0.1);
  }

  return risk;
}

double ContextFusionEngine::EvalNetworkRisk(
    const ProfileSnapshot& p) const {
  double risk = 0.0;

  if (p.network_status == "disconnected") {
    risk = 0.7;
  }

  // Drop count amplifier
  if (p.network_drop_count >= 3) {
    risk = std::min(1.0, risk + 0.3);
  } else if (p.network_drop_count >= 1) {
    risk = std::min(1.0, risk + 0.1);
  }

  return risk;
}

double ContextFusionEngine::EvalAnomalyRisk(
    const ProfileSnapshot& p) const {
  if (p.anomalies.empty()) return 0.0;

  double risk = 0.0;
  for (const auto& a : p.anomalies) {
    std::string sev =
        a.value("severity", "info");
    if (sev == "critical") {
      risk += 0.5;
    } else if (sev == "warning") {
      risk += 0.3;
    } else {
      risk += 0.1;
    }
  }

  return std::min(1.0, risk);
}

SituationLevel ContextFusionEngine::ScoreToLevel(
    double score) {
  if (score >= 0.7) return SituationLevel::kCritical;
  if (score >= 0.4) return SituationLevel::kWarning;
  if (score >= 0.2) return SituationLevel::kAdvisory;
  return SituationLevel::kNormal;
}

std::string ContextFusionEngine::LevelToString(
    SituationLevel level) {
  switch (level) {
    case SituationLevel::kNormal:
      return "normal";
    case SituationLevel::kAdvisory:
      return "advisory";
    case SituationLevel::kWarning:
      return "warning";
    case SituationLevel::kCritical:
      return "critical";
  }
  return "unknown";
}

nlohmann::json ContextFusionEngine::ToJson(
    const SituationAssessment& a) {
  nlohmann::json j;
  j["level"] = LevelToString(a.level);
  j["level_num"] = static_cast<int>(a.level);
  j["risk_score"] =
      std::round(a.risk_score * 100) / 100.0;
  j["summary"] = a.summary;
  j["factors"] = a.factors;
  j["suggestions"] = a.suggestions;
  return j;
}

}  // namespace tizenclaw
