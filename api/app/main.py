from __future__ import annotations

from contextlib import asynccontextmanager

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from starlette.requests import Request

from .db import get_database
from .logging_ import RequestLogMiddleware, configure_logging, log
from .routes import contexts, events, meta, notes, snapshot, todos
from .services.bootstrap import ensure_bootstrap
from .settings import get_settings


@asynccontextmanager
async def lifespan(app: FastAPI):  # noqa: ARG001
    configure_logging()
    log.info("startup.begin")
    db = get_database()
    async with db.session() as session:
        await ensure_bootstrap(session)
    log.info("startup.done")
    try:
        yield
    finally:
        log.info("shutdown.begin")
        await db.dispose()


def create_app() -> FastAPI:
    settings = get_settings()
    app = FastAPI(title="todo-api", version=settings.version, lifespan=lifespan)

    app.add_middleware(
        CORSMiddleware,
        allow_origins=settings.cors_origins,
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
        expose_headers=["Last-Modified", "x-request-id"],
    )
    app.add_middleware(RequestLogMiddleware)

    @app.exception_handler(HTTPException)
    async def _http_exc_handler(_: Request, exc: HTTPException) -> JSONResponse:
        return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail})

    app.include_router(meta.router)
    app.include_router(contexts.router)
    app.include_router(todos.router)
    app.include_router(notes.router)
    app.include_router(events.router)
    app.include_router(snapshot.router)
    return app


app = create_app()
