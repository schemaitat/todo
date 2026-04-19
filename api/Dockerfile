FROM python:3.12-slim AS base

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1

RUN apt-get update \
    && apt-get install -y --no-install-recommends libpq5 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 1000 app
WORKDIR /srv/app

COPY --from=ghcr.io/astral-sh/uv:0.9.16 /uv /usr/local/bin/uv

COPY pyproject.toml ./
RUN uv pip install --system --requirement pyproject.toml

COPY app ./app
COPY alembic.ini ./
COPY alembic ./alembic

USER app
EXPOSE 8000

CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8000"]
