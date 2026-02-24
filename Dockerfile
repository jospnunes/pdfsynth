FROM rustlang/rust:nightly-bookworm AS deps

WORKDIR /app

COPY Cargo.toml Cargo.lock* ./

RUN mkdir src && echo "fn main() {}" > src/main.rs

RUN cargo build --release && rm -rf src target/release/pdfsynth target/release/deps/pdfsynth*

FROM deps AS builder

COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium \
    ghostscript \
    dumb-init \
    fonts-liberation \
    fontconfig \
    icc-profiles-free \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

RUN useradd -m -u 1000 -U appuser

WORKDIR /app

COPY --from=builder /app/target/release/pdfsynth /app/pdfsynth

COPY assets ./assets
COPY fonts ./fonts

RUN cp /usr/share/color/icc/sRGB.icc /app/assets/srgb.icc

RUN fc-cache -f -v

RUN chown -R appuser:appuser /app

USER appuser

EXPOSE 8080

ENTRYPOINT ["/usr/bin/dumb-init", "--"]
CMD ["/app/pdfsynth"]
