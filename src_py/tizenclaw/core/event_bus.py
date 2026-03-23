"""
TizenClaw EventBus — async pub/sub for internal events.
"""
import asyncio
import logging
import time
from typing import Dict, List, Any, Callable, Awaitable, Optional
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


@dataclass
class Event:
    topic: str
    data: Dict[str, Any]
    timestamp: float = field(default_factory=time.time)
    source: str = ""


class EventBus:
    """Async event bus for TizenClaw internal communication."""

    def __init__(self):
        self._subscribers: Dict[str, List[Callable]] = {}
        self._lock = asyncio.Lock()
        self._history: List[Event] = []
        self._max_history = 1000

    async def subscribe(self, topic: str, callback: Callable[[Event], Awaitable[None]]):
        async with self._lock:
            if topic not in self._subscribers:
                self._subscribers[topic] = []
            self._subscribers[topic].append(callback)
            logger.debug(f"EventBus: subscribed to '{topic}'")

    async def unsubscribe(self, topic: str, callback: Callable):
        async with self._lock:
            if topic in self._subscribers:
                self._subscribers[topic] = [cb for cb in self._subscribers[topic] if cb != callback]

    async def publish(self, event: Event):
        """Publish event to all subscribers of the topic."""
        self._history.append(event)
        if len(self._history) > self._max_history:
            self._history = self._history[-self._max_history:]

        subscribers = self._subscribers.get(event.topic, [])
        # Also notify wildcard subscribers
        subscribers += self._subscribers.get("*", [])

        for cb in subscribers:
            try:
                await cb(event)
            except Exception as e:
                logger.error(f"EventBus: handler error for '{event.topic}': {e}")

    async def publish_fire_and_forget(self, event: Event):
        """Publish without waiting for handlers to complete."""
        asyncio.create_task(self.publish(event))

    def get_history(self, topic: Optional[str] = None, limit: int = 50) -> List[Dict]:
        events = self._history
        if topic:
            events = [e for e in events if e.topic == topic]
        return [{"topic": e.topic, "data": e.data, "timestamp": e.timestamp, "source": e.source}
                for e in events[-limit:]]


# Global singleton
_event_bus: Optional[EventBus] = None

def get_event_bus() -> EventBus:
    global _event_bus
    if _event_bus is None:
        _event_bus = EventBus()
    return _event_bus
