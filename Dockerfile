FROM python:3.13-slim

# Install uv
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

ENV UV_COMPILE_BYTECODE=1 \
    UV_SYSTEM_PYTHON=1

WORKDIR /app

# Install dependencies
COPY . .
RUN uv pip install --no-cache -r requirements.txt

CMD ["uv", "run", "main.py"]
