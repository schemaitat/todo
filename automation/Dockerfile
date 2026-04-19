FROM python:3.12-slim

WORKDIR /app

COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv

ENV PREFECT_HOME=/app/.prefect

COPY pyproject.toml .
RUN uv pip install --system --no-cache prefect httpx

COPY entrypoint.sh .
RUN chmod +x entrypoint.sh
COPY flows/ flows/

CMD ["./entrypoint.sh"]
