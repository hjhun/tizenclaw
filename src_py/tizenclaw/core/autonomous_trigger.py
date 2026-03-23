"""
TizenClaw Autonomous Trigger — rule-based automatic responses to system events.

Loads trigger rules from autonomous_trigger.json and subscribes to EventBus.
When an event matches a rule's conditions, it either:
  - "evaluate": forwards to AgentCore for LLM analysis
  - "direct": executes a predefined prompt immediately
"""
import asyncio
import json
import logging
import os
import time
from typing import Dict, List, Any, Optional

logger = logging.getLogger(__name__)

CONFIG_PATH = "/opt/usr/share/tizenclaw/config/autonomous_trigger.json"


class TriggerRule:
    """A single trigger rule definition."""
    def __init__(self, name: str, event_type: str, condition: Dict,
                 action: str, cooldown_minutes: int = 10,
                 direct_prompt: str = ""):
        self.name = name
        self.event_type = event_type
        self.condition = condition
        self.action = action  # "evaluate" or "direct"
        self.cooldown_minutes = cooldown_minutes
        self.direct_prompt = direct_prompt
        self.last_fired: float = 0.0


class AutonomousTrigger:
    """Event-driven autonomous trigger engine."""

    def __init__(self, agent_core=None, notification_callback=None):
        self._agent = agent_core
        self._notify = notification_callback
        self._rules: List[TriggerRule] = []
        self._enabled = False
        self._max_evals_per_hour = 10
        self._eval_count = 0
        self._eval_hour = 0
        self._running = False
        self._notification_channel = "telegram"

    # ── Configuration ──

    def load_rules(self, path: str = CONFIG_PATH) -> bool:
        """Load trigger rules from JSON config file."""
        if not os.path.isfile(path):
            logger.info(f"AutonomousTrigger: Config not found at {path}")
            return False

        try:
            with open(path, "r", encoding="utf-8") as f:
                config = json.load(f)
        except Exception as e:
            logger.error(f"AutonomousTrigger: Failed to load {path}: {e}")
            return False

        self._enabled = config.get("enabled", False)
        self._max_evals_per_hour = config.get("max_evaluations_per_hour", 10)
        self._notification_channel = config.get("notification_channel", "telegram")

        self._rules = []
        for rule_data in config.get("trigger_rules", []):
            name = rule_data.get("name", "")
            event_type = rule_data.get("event_type", "")
            if not name or not event_type:
                continue  # Skip invalid rules
            self._rules.append(TriggerRule(
                name=name,
                event_type=event_type,
                condition=rule_data.get("condition", {}),
                action=rule_data.get("action", "evaluate"),
                cooldown_minutes=rule_data.get("cooldown_minutes", 10),
                direct_prompt=rule_data.get("direct_prompt", ""),
            ))

        logger.info(f"AutonomousTrigger: Loaded {len(self._rules)} rules, enabled={self._enabled}")
        return True

    def is_enabled(self) -> bool:
        return self._enabled

    def list_rules(self) -> List[Dict[str, Any]]:
        return [
            {
                "name": r.name,
                "event_type": r.event_type,
                "condition": r.condition,
                "cooldown_minutes": r.cooldown_minutes,
                "action": r.action,
                "last_fired": r.last_fired,
            }
            for r in self._rules
        ]

    # ── Event Bus integration ──

    async def start(self, event_bus=None):
        """Start listening for events on the EventBus."""
        if not self._enabled:
            logger.info("AutonomousTrigger: Disabled, not starting")
            return

        self._running = True

        if event_bus:
            # Subscribe to all events
            await event_bus.subscribe("*", self._on_event)
            logger.info("AutonomousTrigger: Subscribed to EventBus (wildcard)")

        logger.info("AutonomousTrigger: Started")

    async def stop(self):
        self._running = False
        logger.info("AutonomousTrigger: Stopped")

    # ── Event handling ──

    async def _on_event(self, event):
        """Handle an incoming event from EventBus."""
        if not self._running:
            return

        for rule in self._rules:
            if self._event_matches_rule(event, rule):
                await self._fire_rule(rule, event)

    def _event_matches_rule(self, event, rule: TriggerRule) -> bool:
        """Check if an event matches a rule's event_type and conditions."""
        # Match event type (supports prefix matching)
        event_name = getattr(event, 'topic', '') or ''
        if not event_name.startswith(rule.event_type.split('.')[0]):
            return False

        # Check cooldown
        now = time.time()
        if now - rule.last_fired < rule.cooldown_minutes * 60:
            return False

        # Check rate limit
        current_hour = int(now / 3600)
        if current_hour != self._eval_hour:
            self._eval_hour = current_hour
            self._eval_count = 0
        if self._eval_count >= self._max_evals_per_hour:
            return False

        # Check conditions against event data
        event_data = getattr(event, 'data', {}) or {}
        return self._evaluate_condition(rule.condition, event_data)

    @staticmethod
    def _evaluate_condition(condition: Dict, data: Dict) -> bool:
        """Evaluate MongoDB-style condition operators against event data."""
        if not condition:
            return True  # Empty condition = always match

        for key, spec in condition.items():
            value = data.get(key)
            if value is None:
                return False

            if isinstance(spec, dict):
                for op, threshold in spec.items():
                    if op == "$lt" and not (value < threshold):
                        return False
                    elif op == "$gt" and not (value > threshold):
                        return False
                    elif op == "$eq" and not (value == threshold):
                        return False
                    elif op == "$gte" and not (value >= threshold):
                        return False
                    elif op == "$lte" and not (value <= threshold):
                        return False
            else:
                if value != spec:
                    return False

        return True

    async def _fire_rule(self, rule: TriggerRule, event):
        """Execute a triggered rule."""
        now = time.time()
        rule.last_fired = now
        self._eval_count += 1

        event_data = getattr(event, 'data', {}) or {}
        logger.info(f"AutonomousTrigger: Rule '{rule.name}' fired "
                    f"(action={rule.action}, data={json.dumps(event_data)[:100]})")

        if rule.action == "direct" and rule.direct_prompt:
            # Execute predefined prompt
            if self._agent:
                try:
                    response = await self._agent.process_prompt(
                        "autonomous", rule.direct_prompt
                    )
                    if self._notify:
                        await self._notify(
                            f"🤖 자동 알림 [{rule.name}]\n\n{response}"
                        )
                except Exception as e:
                    logger.error(f"AutonomousTrigger: direct action failed: {e}")

        elif rule.action == "evaluate":
            # Generate a contextual prompt from the event
            prompt = (
                f"시스템 이벤트가 발생했습니다:\n"
                f"- 이벤트: {rule.event_type}\n"
                f"- 데이터: {json.dumps(event_data, ensure_ascii=False)}\n"
                f"- 규칙: {rule.name}\n\n"
                f"이 상황을 분석하고 사용자에게 알릴 필요가 있다면 알려주세요."
            )
            if self._agent:
                try:
                    response = await self._agent.process_prompt(
                        "autonomous", prompt
                    )
                    if self._notify:
                        await self._notify(
                            f"🔔 상태 알림 [{rule.name}]\n\n{response}"
                        )
                except Exception as e:
                    logger.error(f"AutonomousTrigger: evaluate action failed: {e}")

    def get_status(self) -> Dict[str, Any]:
        return {
            "enabled": self._enabled,
            "running": self._running,
            "rules_count": len(self._rules),
            "eval_count_this_hour": self._eval_count,
            "max_evals_per_hour": self._max_evals_per_hour,
            "notification_channel": self._notification_channel,
        }
