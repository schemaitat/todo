from __future__ import annotations

from functools import lru_cache

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore", case_sensitive=False)

    database_url: str = Field(default="sqlite+aiosqlite:///./todo.db")
    allowed_origins: str = Field(default="*")

    bootstrap_user_email: str = Field(default="admin@example.com")
    bootstrap_user_display_name: str = Field(default="admin")
    bootstrap_api_key: str | None = Field(default=None)
    bootstrap_api_key_label: str = Field(default="bootstrap")

    oidc_issuer: str | None = Field(default=None)
    oidc_client_id: str | None = Field(default=None)

    version: str = Field(default="0.1.0")
    log_level: str = Field(default="INFO")
    log_json: bool = Field(default=True)

    @property
    def cors_origins(self) -> list[str]:
        value = self.allowed_origins.strip()
        if value in ("", "*"):
            return ["*"]
        return [o.strip() for o in value.split(",") if o.strip()]


@lru_cache(maxsize=1)
def get_settings() -> Settings:
    return Settings()
