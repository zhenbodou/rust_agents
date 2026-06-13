FROM ghcr.io/astral-sh/uv:0.9.5-python3.12-trixie-slim AS build
WORKDIR /app
ENV UV_COMPILE_BYTECODE=1 \
    UV_LINK_MODE=copy
COPY pyproject.toml uv.lock ./
COPY src/ src/
RUN uv sync --locked --no-dev

FROM ghcr.io/astral-sh/uv:0.9.5-python3.12-trixie-slim AS runtime
RUN useradd -m -u 10001 app
COPY --from=build --chown=app:app /app /app
USER app
ENV PATH="/app/.venv/bin:$PATH"
ENV PYTHONUNBUFFERED=1
ENTRYPOINT ["python", "-m", "runner.main"]
