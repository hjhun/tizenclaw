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
#include "autonomous_trigger.hh"

#include <chrono>
#include <fstream>
#include <thread>

#include "../../common/logging.hh"
#include "agent_core.hh"

namespace tizenclaw {

namespace {

int64_t NowMs() {
  return std::chrono::duration_cast<
             std::chrono::milliseconds>(
             std::chrono::system_clock::now()
                 .time_since_epoch())
      .count();
}

int64_t NowMinutes() {
  return NowMs() / 60000;
}

}  // namespace

AutonomousTrigger::AutonomousTrigger(
    AgentCore* agent,
    SystemContextProvider* context,
    ChannelRegistry* channels)
    : agent_(agent),
      context_(context),
      channels_(channels) {}

AutonomousTrigger::~AutonomousTrigger() {
  Stop();
}

bool AutonomousTrigger::LoadRules(
    const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(WARNING) << "AutonomousTrigger: config "
                 << "not found: " << config_path;
    return false;
  }

  try {
    nlohmann::json config;
    f >> config;
    f.close();

    enabled_ = config.value("enabled", false);
    eval_session_ = config.value(
        "evaluation_session", "autonomous");
    max_evals_per_hour_ = config.value(
        "max_evaluations_per_hour", 10);
    notification_channel_ = config.value(
        "notification_channel", "telegram");

    std::lock_guard<std::mutex> lock(rules_mutex_);
    rules_.clear();

    if (config.contains("trigger_rules") &&
        config["trigger_rules"].is_array()) {
      for (const auto& r : config["trigger_rules"]) {
        EventRule rule;
        rule.name = r.value("name", "");
        rule.event_type =
            r.value("event_type", "");
        rule.cooldown_minutes =
            r.value("cooldown_minutes", 10);
        rule.action = r.value("action", "evaluate");
        rule.direct_prompt =
            r.value("direct_prompt", "");

        if (r.contains("condition")) {
          rule.condition = r["condition"];
        }

        if (rule.name.empty() ||
            rule.event_type.empty()) {
          LOG(WARNING) << "AutonomousTrigger: "
                       << "skipping rule with "
                       << "empty name/event_type";
          continue;
        }

        rules_.push_back(std::move(rule));
      }
    }

    LOG(INFO) << "AutonomousTrigger: loaded "
              << rules_.size() << " rules "
              << "(enabled=" << enabled_ << ")";
    return true;
  } catch (const std::exception& e) {
    LOG(ERROR) << "AutonomousTrigger: parse "
               << "error: " << e.what();
    return false;
  }
}

void AutonomousTrigger::Start() {
  if (started_ || !enabled_) return;

  subscription_id_ =
      EventBus::GetInstance().SubscribeAll(
          [this](const SystemEvent& event) {
            OnEvent(event);
          });

  started_ = true;
  LOG(INFO) << "AutonomousTrigger started "
            << "(" << rules_.size()
            << " rules active)";
}

void AutonomousTrigger::Stop() {
  if (!started_) return;

  if (subscription_id_ >= 0) {
    EventBus::GetInstance().Unsubscribe(
        subscription_id_);
    subscription_id_ = -1;
  }

  started_ = false;
  LOG(INFO) << "AutonomousTrigger stopped";
}

nlohmann::json AutonomousTrigger::ListRules() const {
  std::lock_guard<std::mutex> lock(rules_mutex_);

  auto result = nlohmann::json::array();
  for (const auto& rule : rules_) {
    result.push_back({
        {"name", rule.name},
        {"event_type", rule.event_type},
        {"condition", rule.condition},
        {"cooldown_minutes", rule.cooldown_minutes},
        {"action", rule.action}});
  }
  return result;
}

void AutonomousTrigger::OnEvent(
    const SystemEvent& event) {
  std::lock_guard<std::mutex> lock(rules_mutex_);

  for (const auto& rule : rules_) {
    if (!MatchRule(rule, event)) continue;

    if (IsCoolingDown(rule.name)) {
      LOG(DEBUG) << "AutonomousTrigger: rule '"
                 << rule.name
                 << "' in cooldown, skipping";
      continue;
    }

    RecordTrigger(rule.name);

    if (rule.action == "direct") {
      // Direct action: skip LLM evaluation
      std::string prompt = rule.direct_prompt;
      if (prompt.empty()) {
        prompt = "Event '" + event.name + "' "
                 "occurred. Please respond.";
      }

      LOG(INFO) << "AutonomousTrigger: direct "
                << "action for rule '"
                << rule.name << "'";

      // Execute in a detached thread to
      // avoid blocking EventBus dispatch
      std::thread([this, prompt, rule]() {
        ExecuteAction("execute", prompt,
                      "direct_rule:" + rule.name);
      }).detach();

    } else if (rule.action == "evaluate") {
      if (!CheckRateLimit()) {
        LOG(WARNING) << "AutonomousTrigger: "
                     << "rate limit reached";
        continue;
      }

      LOG(INFO) << "AutonomousTrigger: "
                << "evaluating rule '"
                << rule.name << "' via LLM";

      // Evaluate in a detached thread
      std::thread(
          [this, rule, event]() {
            EvaluateWithLlm(rule, event);
          })
          .detach();
    }
  }
}

bool AutonomousTrigger::MatchRule(
    const EventRule& rule,
    const SystemEvent& event) const {
  // Match event type (name)
  if (event.name != rule.event_type) return false;

  // If no conditions, match any event of this type
  if (rule.condition.empty()) return true;

  return EvalCondition(rule.condition, event.data);
}

bool AutonomousTrigger::EvalCondition(
    const nlohmann::json& condition,
    const nlohmann::json& data) const {
  // For each key in condition, check against data
  for (auto& [key, ops] : condition.items()) {
    if (!data.contains(key)) return false;

    const auto& val = data[key];

    if (!ops.is_object()) {
      // Direct equality check
      if (val != ops) return false;
      continue;
    }

    for (auto& [op, expected] : ops.items()) {
      if (op == "$eq" && val != expected)
        return false;
      if (op == "$lt") {
        if (val.is_number() &&
            expected.is_number()) {
          if (val.get<double>() >=
              expected.get<double>())
            return false;
        }
      }
      if (op == "$gt") {
        if (val.is_number() &&
            expected.is_number()) {
          if (val.get<double>() <=
              expected.get<double>())
            return false;
        }
      }
      if (op == "$lte") {
        if (val.is_number() &&
            expected.is_number()) {
          if (val.get<double>() >
              expected.get<double>())
            return false;
        }
      }
      if (op == "$gte") {
        if (val.is_number() &&
            expected.is_number()) {
          if (val.get<double>() <
              expected.get<double>())
            return false;
        }
      }
    }
  }

  return true;
}

bool AutonomousTrigger::IsCoolingDown(
    const std::string& rule_name) const {
  std::lock_guard<std::mutex> lock(
      cooldown_mutex_);
  auto it = last_trigger_.find(rule_name);
  if (it == last_trigger_.end()) return false;

  // Find the rule's cooldown
  int cooldown = 10;  // default
  for (const auto& rule : rules_) {
    if (rule.name == rule_name) {
      cooldown = rule.cooldown_minutes;
      break;
    }
  }

  return (NowMinutes() - it->second) < cooldown;
}

void AutonomousTrigger::RecordTrigger(
    const std::string& rule_name) {
  std::lock_guard<std::mutex> lock(
      cooldown_mutex_);
  last_trigger_[rule_name] = NowMinutes();
}

bool AutonomousTrigger::CheckRateLimit() {
  std::lock_guard<std::mutex> lock(rate_mutex_);
  int64_t now = NowMinutes();
  int64_t current_hour = now / 60;

  if (current_hour != hour_start_ / 60) {
    // New hour, reset counter
    hour_start_ = now;
    eval_count_ = 0;
  }

  if (eval_count_ >= max_evals_per_hour_) {
    return false;
  }

  eval_count_++;
  return true;
}

void AutonomousTrigger::EvaluateWithLlm(
    const EventRule& rule,
    const SystemEvent& event) {
  if (!agent_) return;

  // Build evaluation prompt
  std::string event_json = event.data.dump();
  std::string system_ctx;
  if (context_) {
    system_ctx = context_->GetContextString();
  }

  std::string eval_prompt =
      "다음 시스템 이벤트가 발생했습니다.\n\n"
      "규칙: " + rule.name + "\n"
      "이벤트: " + event.name + "\n"
      "데이터: " + event_json + "\n\n"
      "현재 시스템 상태:\n" + system_ctx + "\n\n"
      "이 이벤트에 대한 대응이 필요한지 판단하고, "
      "필요하다면 사용자에게 알리거나 적절한 조치를 "
      "취해주세요. 불필요하다면 무시해주세요.";

  try {
    auto result = agent_->ProcessPrompt(
        eval_prompt, eval_session_);

    // Log the result
    LOG(INFO) << "AutonomousTrigger: LLM eval "
              << "for '" << rule.name
              << "' completed";

    // Notify user of the result
    if (!result.empty()) {
      Notify("[🤖 자율 판단] " + rule.name + "\n"
             + result);
    }
  } catch (const std::exception& e) {
    LOG(ERROR) << "AutonomousTrigger: LLM "
               << "evaluation failed: "
               << e.what();
  }
}

void AutonomousTrigger::ExecuteAction(
    const std::string& action,
    const std::string& prompt,
    const std::string& reason) {
  if (!agent_) return;

  LOG(INFO) << "AutonomousTrigger: executing "
            << "action='" << action
            << "' reason='" << reason << "'";

  try {
    auto result = agent_->ProcessPrompt(
        prompt, eval_session_);

    if (!result.empty()) {
      Notify("[🤖 자율 조치] " + reason + "\n"
             + result);
    }
  } catch (const std::exception& e) {
    LOG(ERROR) << "AutonomousTrigger: execution "
               << "failed: " << e.what();
  }
}

void AutonomousTrigger::Notify(
    const std::string& message) {
  if (!agent_) return;

  LOG(INFO) << "AutonomousTrigger: notify via "
            << notification_channel_;

  if (channels_) {
    if (!channels_->SendTo(
            notification_channel_, message)) {
      LOG(WARNING) << "Failed to send via "
                   << notification_channel_
                   << ", broadcasting";
      channels_->Broadcast(message);
    }
  }
}

}  // namespace tizenclaw
