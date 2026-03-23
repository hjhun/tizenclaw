"""
TizenClaw Perception Engine — proactive device situation awareness.

Three-stage pipeline:
  1. DeviceProfiler: collects and analyzes system events into a ProfileSnapshot
  2. ContextFusionEngine: fuses snapshot data into a SituationAssessment with risk score
  3. ProactiveAdvisor: decides whether to suppress, notify, or evaluate based on assessment

Runs on a periodic analysis tick (default 30s) and publishes perception events.
"""
import asyncio
import json
import logging
import time
from enum import IntEnum
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


# ── Data structures ────────────────────────────────────────────────


class SituationLevel(IntEnum):
    NORMAL = 0
    ADVISORY = 1
    WARNING = 2
    CRITICAL = 3


class AdvisoryAction(IntEnum):
    SUPPRESS = 0
    INJECT = 1   # inject context into next LLM call
    NOTIFY = 2   # send notification to user
    EVALUATE = 3 # trigger AgentCore evaluation


@dataclass
class ProfileSnapshot:
    battery_level: int = 100
    charging: bool = False
    battery_drain_rate: float = 0.0
    battery_health: str = "good"
    memory_trend: str = "stable"
    memory_warning_count: int = 0
    network_status: str = "connected"
    network_drop_count: int = 0
    foreground_app: str = ""
    timestamp: float = field(default_factory=time.time)


@dataclass
class SituationAssessment:
    level: SituationLevel = SituationLevel.NORMAL
    risk_score: float = 0.0
    summary: str = ""
    factors: List[str] = field(default_factory=list)
    suggestions: List[str] = field(default_factory=list)


@dataclass
class Advisory:
    action: AdvisoryAction = AdvisoryAction.SUPPRESS
    message: str = ""
    assessment: Optional[SituationAssessment] = None


# ── DeviceProfiler ─────────────────────────────────────────────────


class DeviceProfiler:
    """Collects system events and produces ProfileSnapshots."""

    def __init__(self):
        self._events: List[Dict[str, Any]] = []
        self._max_events = 500
        self._lock = asyncio.Lock()

    async def record_event(self, event):
        """Record a system event from the EventBus."""
        data = getattr(event, 'data', {}) or {}
        topic = getattr(event, 'topic', '') or ''
        async with self._lock:
            self._events.append({
                "topic": topic,
                "data": data,
                "timestamp": time.time(),
            })
            if len(self._events) > self._max_events:
                self._events = self._events[-self._max_events:]

    def get_event_count(self) -> int:
        return len(self._events)

    def analyze(self) -> ProfileSnapshot:
        """Analyze collected events into a snapshot."""
        snap = ProfileSnapshot()
        now = time.time()
        window = 300  # 5 minute analysis window

        for ev in reversed(self._events):
            if now - ev.get("timestamp", 0) > window:
                break
            topic = ev.get("topic", "")
            data = ev.get("data", {})

            if "battery" in topic:
                level = data.get("level", data.get("percent"))
                if level is not None:
                    snap.battery_level = int(level)
                charging = data.get("charging", data.get("is_charging"))
                if charging is not None:
                    snap.charging = bool(charging)

            elif "memory" in topic:
                level = data.get("level", "")
                if level:
                    snap.memory_trend = str(level)
                snap.memory_warning_count += 1

            elif "network" in topic:
                status = data.get("status", "")
                if status:
                    snap.network_status = status
                if "disconnect" in topic or status == "disconnected":
                    snap.network_drop_count += 1

            elif "app" in topic:
                app_id = data.get("app_id", "")
                if app_id:
                    snap.foreground_app = app_id

        # Classify battery health
        if snap.charging:
            snap.battery_health = "charging"
        elif snap.battery_level <= 5:
            snap.battery_health = "critical"
        elif snap.battery_level <= 15:
            snap.battery_health = "degrading"
        else:
            snap.battery_health = "good"

        return snap


# ── ContextFusionEngine ────────────────────────────────────────────


class ContextFusionEngine:
    """Fuses profile snapshot data into a SituationAssessment."""

    def fuse(self, snap: ProfileSnapshot,
             extra_context: Dict[str, Any] = None) -> SituationAssessment:
        """Evaluate risk from multiple dimensions."""
        assessment = SituationAssessment()
        risk = 0.0
        factors = []
        suggestions = []

        # ── Battery risk ──
        if not snap.charging:
            if snap.battery_level <= 5:
                risk += 0.4
                factors.append(f"배터리 위험: {snap.battery_level}%")
                suggestions.append("충전기를 연결하세요")
            elif snap.battery_level <= 15:
                risk += 0.25
                factors.append(f"배터리 부족: {snap.battery_level}%")
                suggestions.append("불필요한 앱을 종료하세요")
            elif snap.battery_level <= 30:
                risk += 0.1
                factors.append(f"배터리 주의: {snap.battery_level}%")

        # ── Memory risk ──
        if snap.memory_trend == "critical":
            risk += 0.3
            factors.append("메모리 위험 수준")
            suggestions.append("메모리를 많이 사용하는 프로세스를 종료하세요")
        elif snap.memory_warning_count >= 3:
            risk += 0.2
            factors.append(f"메모리 경고 {snap.memory_warning_count}회")

        # ── Network risk ──
        if snap.network_status == "disconnected":
            risk += 0.15
            factors.append("네트워크 연결 끊김")
            suggestions.append("Wi-Fi 또는 모바일 데이터 연결을 확인하세요")
        elif snap.network_drop_count >= 3:
            risk += 0.1
            factors.append(f"네트워크 불안정 (끊김 {snap.network_drop_count}회)")

        # ── Determine level ──
        risk = min(risk, 1.0)
        assessment.risk_score = risk
        assessment.factors = factors
        assessment.suggestions = suggestions

        if risk >= 0.7:
            assessment.level = SituationLevel.CRITICAL
            assessment.summary = "시스템이 위험 상태입니다"
        elif risk >= 0.4:
            assessment.level = SituationLevel.WARNING
            assessment.summary = "주의가 필요한 상태입니다"
        elif risk >= 0.2:
            assessment.level = SituationLevel.ADVISORY
            assessment.summary = "경미한 문제가 감지되었습니다"
        else:
            assessment.level = SituationLevel.NORMAL
            assessment.summary = "정상 상태입니다"

        return assessment

    @staticmethod
    def level_to_string(level: SituationLevel) -> str:
        return {
            SituationLevel.NORMAL: "normal",
            SituationLevel.ADVISORY: "advisory",
            SituationLevel.WARNING: "warning",
            SituationLevel.CRITICAL: "critical",
        }.get(level, "unknown")

    @staticmethod
    def to_json(assessment: SituationAssessment) -> Dict[str, Any]:
        return {
            "level": ContextFusionEngine.level_to_string(assessment.level),
            "level_num": int(assessment.level),
            "risk_score": assessment.risk_score,
            "summary": assessment.summary,
            "factors": assessment.factors,
            "suggestions": assessment.suggestions,
        }


# ── ProactiveAdvisor ───────────────────────────────────────────────


class ProactiveAdvisor:
    """Decides what action to take based on SituationAssessment."""

    def __init__(self, agent_core=None, event_bus=None):
        self._agent = agent_core
        self._event_bus = event_bus
        self._last_insight: Dict[str, Any] = {}
        self._last_notify_time: float = 0
        self._cooldown = 120  # seconds between notifications

    def evaluate(self, assessment: SituationAssessment) -> Advisory:
        """Determine action based on assessment level."""
        self._last_insight = ContextFusionEngine.to_json(assessment)
        advisory = Advisory(assessment=assessment)

        if assessment.level == SituationLevel.NORMAL:
            advisory.action = AdvisoryAction.SUPPRESS
            return advisory

        # Check cooldown
        now = time.time()
        if now - self._last_notify_time < self._cooldown:
            advisory.action = AdvisoryAction.INJECT
            advisory.message = assessment.summary
            return advisory

        if assessment.level == SituationLevel.CRITICAL:
            advisory.action = AdvisoryAction.EVALUATE
            advisory.message = (
                f"⚠️ {assessment.summary}\n"
                f"위험 요인: {', '.join(assessment.factors)}\n"
                f"권장 조치: {', '.join(assessment.suggestions)}"
            )
            self._last_notify_time = now
        elif assessment.level >= SituationLevel.WARNING:
            advisory.action = AdvisoryAction.NOTIFY
            advisory.message = (
                f"⚠️ {assessment.summary}\n"
                f"요인: {', '.join(assessment.factors)}"
            )
            self._last_notify_time = now
        else:
            advisory.action = AdvisoryAction.INJECT
            advisory.message = assessment.summary

        return advisory

    def get_last_insight(self) -> Dict[str, Any]:
        return self._last_insight


# ── PerceptionEngine (integration) ─────────────────────────────────


class PerceptionEngine:
    """
    Main perception engine that ties DeviceProfiler, ContextFusionEngine,
    and ProactiveAdvisor together with a periodic analysis loop.
    """

    ANALYSIS_INTERVAL = 30  # seconds

    def __init__(self, agent_core=None, event_bus=None,
                 notification_callback=None):
        self._agent = agent_core
        self._event_bus = event_bus
        self._notify = notification_callback
        self._profiler = DeviceProfiler()
        self._fusion = ContextFusionEngine()
        self._advisor = ProactiveAdvisor(agent_core, event_bus)
        self._running = False
        self._task: Optional[asyncio.Task] = None

    async def start(self):
        """Start the perception engine: subscribe to events + analysis loop."""
        self._running = True

        # Subscribe to all events for profiling
        if self._event_bus:
            await self._event_bus.subscribe("*", self._profiler.record_event)

        self._task = asyncio.create_task(self._analysis_loop())
        logger.info("PerceptionEngine: Started")

    async def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass
        logger.info("PerceptionEngine: Stopped")

    def is_running(self) -> bool:
        return self._running

    def get_insight(self) -> Dict[str, Any]:
        return self._advisor.get_last_insight()

    async def _analysis_loop(self):
        """Periodic analysis tick."""
        while self._running:
            try:
                await asyncio.sleep(self.ANALYSIS_INTERVAL)

                if self._profiler.get_event_count() == 0:
                    continue

                # Run analysis pipeline
                snapshot = self._profiler.analyze()
                assessment = self._fusion.fuse(snapshot)
                advisory = self._advisor.evaluate(assessment)

                # Publish perception event
                if self._event_bus and assessment.level >= SituationLevel.ADVISORY:
                    from tizenclaw.core.event_bus import Event
                    await self._event_bus.publish_fire_and_forget(Event(
                        topic="perception.situation_changed",
                        data=ContextFusionEngine.to_json(assessment),
                        source="perception",
                    ))

                # Take action
                if advisory.action == AdvisoryAction.NOTIFY and self._notify:
                    await self._notify(advisory.message)
                elif advisory.action == AdvisoryAction.EVALUATE:
                    if self._notify:
                        await self._notify(advisory.message)
                    if self._agent:
                        try:
                            prompt = (
                                f"Perception Engine 분석 결과:\n"
                                f"{advisory.message}\n\n"
                                f"현재 디바이스 상태를 확인하고 사용자에게 도움이 되는 조언을 해주세요."
                            )
                            await self._agent.process_prompt("perception", prompt)
                        except Exception as e:
                            logger.error(f"PerceptionEngine: Agent eval failed: {e}")

            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"PerceptionEngine: Analysis error: {e}")

    def get_status(self) -> Dict[str, Any]:
        return {
            "running": self._running,
            "event_count": self._profiler.get_event_count(),
            "last_insight": self._advisor.get_last_insight(),
            "analysis_interval_sec": self.ANALYSIS_INTERVAL,
        }
