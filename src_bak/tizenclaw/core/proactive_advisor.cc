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
#include "proactive_advisor.hh"

#include <chrono>
#include <sstream>
#include <thread>

#include "../../common/logging.hh"
#include "agent_core.hh"

namespace tizenclaw {

namespace {

int64_t NowMinutes() {
  return std::chrono::duration_cast<
             std::chrono::minutes>(
             std::chrono::system_clock::now()
                 .time_since_epoch())
      .count();
}

int64_t NowMs() {
  return std::chrono::duration_cast<
             std::chrono::milliseconds>(
             std::chrono::system_clock::now()
                 .time_since_epoch())
      .count();
}

}  // namespace

ProactiveAdvisor::ProactiveAdvisor(
    AgentCore* agent,
    ChannelRegistry* channels)
    : agent_(agent), channels_(channels) {}

ProactiveAdvisor::~ProactiveAdvisor() {
  JoinEvalThread();
}

void ProactiveAdvisor::JoinEvalThread() {
  eval_running_.store(false);
  if (eval_thread_.joinable())
    eval_thread_.join();
}

Advisory ProactiveAdvisor::Evaluate(
    const SituationAssessment& assessment) {
  Advisory advisory;

  // Always update the last insight for context
  // injection
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    last_insight_ = ContextFusionEngine::ToJson(
        assessment);
    last_level_ = assessment.level;
  }

  // Publish synthetic event to EventBus so
  // AutonomousTrigger rules can also react
  PublishSituationEvent(assessment);

  // Determine action based on situation level
  switch (assessment.level) {
    case SituationLevel::kNormal:
      advisory.action = AdvisoryAction::kSuppress;
      advisory.message = "";
      return advisory;

    case SituationLevel::kAdvisory: {
      std::string key = "advisory";
      if (IsCoolingDown(key)) {
        advisory.action = AdvisoryAction::kInject;
        advisory.message = "";
        return advisory;
      }
      // Advisory: inject into context only
      advisory.action = AdvisoryAction::kInject;
      advisory.context =
          ContextFusionEngine::ToJson(assessment);
      RecordAction(key);
      return advisory;
    }

    case SituationLevel::kWarning: {
      std::string key = "warning";
      if (IsCoolingDown(key)) {
        advisory.action = AdvisoryAction::kInject;
        return advisory;
      }
      // Warning: notify user via channels
      advisory.action = AdvisoryAction::kNotify;
      advisory.message =
          BuildNotification(assessment);
      advisory.channel = notification_channel_;
      advisory.context =
          ContextFusionEngine::ToJson(assessment);
      RecordAction(key);
      return advisory;
    }

    case SituationLevel::kCritical: {
      std::string key = "critical";
      if (IsCoolingDown(key)) {
        advisory.action = AdvisoryAction::kInject;
        return advisory;
      }
      // Critical: notify AND ask LLM to evaluate
      advisory.action = AdvisoryAction::kEvaluate;
      advisory.message =
          BuildNotification(assessment);
      advisory.channel = notification_channel_;
      advisory.context =
          ContextFusionEngine::ToJson(assessment);
      RecordAction(key);
      return advisory;
    }
  }

  advisory.action = AdvisoryAction::kSuppress;
  return advisory;
}

void ProactiveAdvisor::Execute(
    const Advisory& advisory) {
  switch (advisory.action) {
    case AdvisoryAction::kSuppress:
      // Nothing to do
      break;

    case AdvisoryAction::kInject:
      // Context injection happens via
      // GetLastInsight() → SystemContextProvider
      LOG(DEBUG) << "ProactiveAdvisor: context "
                 << "injected (advisory level)";
      break;

    case AdvisoryAction::kNotify:
      // Send notification via channels
      if (!advisory.message.empty() &&
          channels_) {
        LOG(INFO) << "ProactiveAdvisor: sending "
                  << "warning notification via "
                  << advisory.channel;
        if (advisory.channel == "all") {
          channels_->Broadcast(advisory.message);
        } else {
          if (!channels_->SendTo(
                  advisory.channel,
                  advisory.message)) {
            // Fallback to broadcast
            LOG(WARNING)
                << "ProactiveAdvisor: channel '"
                << advisory.channel
                << "' send failed, broadcasting";
            channels_->Broadcast(advisory.message);
          }
        }
      }
      break;

    case AdvisoryAction::kEvaluate:
      // First notify user
      if (!advisory.message.empty() &&
          channels_) {
        LOG(INFO) << "ProactiveAdvisor: sending "
                  << "critical notification";
        if (advisory.channel == "all") {
          channels_->Broadcast(advisory.message);
        } else {
          if (!channels_->SendTo(
                  advisory.channel,
                  advisory.message)) {
            channels_->Broadcast(advisory.message);
          }
        }
      }

      // Then ask LLM to evaluate and suggest
      // actions
      if (agent_) {
        std::string eval_prompt =
            "디바이스가 위험 상태입니다.\n\n"
            "현재 상황:\n" +
            advisory.context.dump(2) +
            "\n\n"
            "즉시 조치가 필요한 사항을 분석하고, "
            "구체적인 해결 방법을 제안해주세요. "
            "가능한 도구가 있다면 직접 실행해주세요.";

        // Join any previous eval thread before
        // starting a new one
        JoinEvalThread();

        eval_running_.store(true);
        eval_thread_ = std::thread(
            [this, eval_prompt]() {
              try {
                if (!eval_running_.load()) return;
                auto result =
                    agent_->ProcessPrompt(
                        "perception", eval_prompt);
                if (!eval_running_.load()) return;
                if (!result.empty() && channels_) {
                  std::string msg =
                      "🧠 [Perception Engine 분석"
                      " 결과]\n" +
                      result;
                  channels_->Broadcast(msg);
                }
              } catch (const std::exception& e) {
                LOG(ERROR)
                    << "ProactiveAdvisor: LLM "
                    << "evaluation failed: "
                    << e.what();
              }
              eval_running_.store(false);
            });
      }
      break;
  }
}

nlohmann::json ProactiveAdvisor::GetLastInsight()
    const {
  std::lock_guard<std::mutex> lock(state_mutex_);
  return last_insight_;
}

bool ProactiveAdvisor::IsCoolingDown(
    const std::string& key) const {
  std::lock_guard<std::mutex> lock(cooldown_mutex_);
  auto it = cooldowns_.find(key);
  if (it == cooldowns_.end()) return false;

  int cooldown_min = kAdvisoryCooldownMin;
  if (key == "warning") {
    cooldown_min = kWarningCooldownMin;
  } else if (key == "critical") {
    cooldown_min = kCriticalCooldownMin;
  }

  return (NowMinutes() - it->second) <
         cooldown_min;
}

void ProactiveAdvisor::RecordAction(
    const std::string& key) {
  std::lock_guard<std::mutex> lock(cooldown_mutex_);
  cooldowns_[key] = NowMinutes();
}

std::string ProactiveAdvisor::BuildNotification(
    const SituationAssessment& assessment) const {
  std::ostringstream ss;

  // Emoji based on level
  switch (assessment.level) {
    case SituationLevel::kWarning:
      ss << "⚠️ [디바이스 경고]\n";
      break;
    case SituationLevel::kCritical:
      ss << "🔴 [디바이스 위험]\n";
      break;
    default:
      ss << "ℹ️ [디바이스 알림]\n";
      break;
  }

  ss << assessment.summary << "\n\n";

  if (!assessment.factors.empty()) {
    ss << "📋 위험 요인:\n";
    for (const auto& f : assessment.factors) {
      ss << "  • " << f << "\n";
    }
    ss << "\n";
  }

  if (!assessment.suggestions.empty()) {
    ss << "💡 제안:\n";
    for (const auto& s : assessment.suggestions) {
      ss << "  • " << s << "\n";
    }
  }

  ss << "\n위험도: "
     << static_cast<int>(
            assessment.risk_score * 100)
     << "%";

  return ss.str();
}

void ProactiveAdvisor::PublishSituationEvent(
    const SituationAssessment& assessment) {
  SystemEvent event;
  event.type = EventType::kCustom;
  event.source = "perception";
  event.name = "perception.situation_changed";
  event.plugin_id = "builtin";
  event.timestamp = NowMs();
  event.data = {
      {"level",
       static_cast<int>(assessment.level)},
      {"level_name",
       ContextFusionEngine::LevelToString(
           assessment.level)},
      {"risk_score", assessment.risk_score},
      {"summary", assessment.summary},
      {"factor_count",
       (int)assessment.factors.size()}};

  EventBus::GetInstance().Publish(
      std::move(event));
}

}  // namespace tizenclaw
