from __future__ import annotations

from fastapi import APIRouter, Depends

from ..auth import get_current_user
from ..models import User
from ..schemas import HealthOut, UserOut, VersionOut
from ..settings import get_settings

router = APIRouter()


@router.get("/health", response_model=HealthOut, tags=["meta"])
async def health() -> HealthOut:
    return HealthOut(status="ok")


@router.get("/version", response_model=VersionOut, tags=["meta"])
async def version() -> VersionOut:
    return VersionOut(version=get_settings().version)


@router.get("/me", response_model=UserOut, tags=["meta"])
async def me(user: User = Depends(get_current_user)) -> User:
    return user
