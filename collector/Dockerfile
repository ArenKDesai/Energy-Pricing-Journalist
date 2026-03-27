FROM python:3.13-slim

COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

ENV UV_COMPILE_BYTECODE=1 \
    UV_SYSTEM_PYTHON=1

WORKDIR /app

COPY requirements.txt .
RUN uv pip install --no-cache -r requirements.txt

COPY . .

CMD ["python", "main.py"]
