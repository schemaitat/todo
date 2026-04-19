from __future__ import annotations

import logging
import sys
import time
from contextvars import ContextVar
from typing import Any
from uuid import uuid4

import structlog
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.types import ASGIApp

from .settings import get_settings

_request_id_var: ContextVar[str] = ContextVar("request_id", default="-")
_user_id_var: ContextVar[str] = ContextVar("user_id", default="-")


def bind_request_id(rid: str) -> None:
    _request_id_var.set(rid)


def bind_user_id(uid: str) -> None:
    _user_id_var.set(uid)


def _inject_context(_, __, event_dict: dict[str, Any]) -> dict[str, Any]:
    event_dict.setdefault("request_id", _request_id_var.get())
    event_dict.setdefault("user_id", _user_id_var.get())
    return event_dict


def configure_logging() -> None:
    settings = get_settings()
    level = getattr(logging, settings.log_level.upper(), logging.INFO)
    logging.basicConfig(stream=sys.stdout, level=level, format="%(message)s")

    processors: list[Any] = [
        structlog.contextvars.merge_contextvars,
        _inject_context,
        structlog.processors.TimeStamper(fmt="iso", utc=True),
        structlog.processors.add_log_level,
    ]
    if settings.log_json:
        processors.append(structlog.processors.JSONRenderer())
    else:
        processors.append(structlog.dev.ConsoleRenderer())

    structlog.configure(
        processors=processors,
        wrapper_class=structlog.make_filtering_bound_logger(level),
        logger_factory=structlog.PrintLoggerFactory(),
        cache_logger_on_first_use=True,
    )


log = structlog.get_logger()


class RequestLogMiddleware(BaseHTTPMiddleware):
    def __init__(self, app: ASGIApp) -> None:
        super().__init__(app)

    async def dispatch(self, request: Request, call_next):  # type: ignore[override]
        rid = request.headers.get("x-request-id") or uuid4().hex[:12]
        bind_request_id(rid)
        bind_user_id("-")
        start = time.perf_counter()
        try:
            response = await call_next(request)
        except Exception:
            log.exception("request.failed", path=request.url.path, method=request.method)
            raise
        latency_ms = (time.perf_counter() - start) * 1000.0
        response.headers["x-request-id"] = rid
        log.info(
            "request",
            path=request.url.path,
            method=request.method,
            status=response.status_code,
            latency_ms=round(latency_ms, 2),
        )
        return response
