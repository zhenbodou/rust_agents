FROM python:3.12-slim AS build
COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv
WORKDIR /app
COPY pyproject.toml ./
COPY src/ src/
RUN uv venv && uv pip install .

FROM python:3.12-slim AS runtime
RUN useradd -m -u 10001 app
COPY --from=build /app /app
USER app
ENV PATH="/app/.venv/bin:$PATH"
ENTRYPOINT ["python", "-m", "runner.main"]
