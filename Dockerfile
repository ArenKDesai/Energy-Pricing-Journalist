FROM python:3.13-slim

COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

# - UV_COMPILE_BYTECODE: Compiles to .pyc for faster startup
# - UV_SYSTEM_PYTHON: Tells uv to install to the system site-packages
ENV UV_COMPILE_BYTECODE=1 \
    UV_SYSTEM_PYTHON=1

WORKDIR /app

COPY requirements.txt .
RUN uv pip install --no-cache -r requirements.txt

COPY . .

RUN apt-get update && apt-get install -y --no-install-recommends \
    vim curl ca-certificates \ 
    && rm -rf /var/lib/apt/lists/*

# The command remains the same
CMD ["python", "main.py"]