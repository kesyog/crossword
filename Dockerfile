# Rust crate build stage
FROM rust:latest as builder
WORKDIR /usr/src/crosswords
COPY Cargo* ./
COPY src/ ./src
RUN cargo install --path .

FROM python:3.9-slim
COPY --from=builder /usr/local/cargo/bin/crossword /usr/local/bin/crossword

# Install Python dependencies
WORKDIR /tmp/build
COPY requirements.txt ./
COPY plot/requirements.txt ./plot/
RUN pip install --no-cache-dir -r requirements.txt

WORKDIR /usr/app
COPY cloud_run.py .
COPY plot/plot.py ./plot/plot.py

CMD exec gunicorn --bind :$PORT --workers 1 --threads 8 --timeout 0 cloud_run:app
