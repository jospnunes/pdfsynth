
FROM rustlang/rust:nightly-bookworm as builder

WORKDIR /app
COPY . .


RUN cargo build --release


FROM debian:bookworm-slim


RUN apt-get update && apt-get install -y \
    chromium \
    ghostscript \
    dumb-init \
    fonts-liberation \
    fontconfig \
    icc-profiles-free \
    && rm -rf /var/lib/apt/lists/*


RUN useradd -m -u 1000 -U appuser

WORKDIR /app


COPY --from=builder /app/target/release/pdfsynth /app/pdfsynth


COPY --from=builder /app/assets /app/assets
COPY --from=builder /app/fonts /app/fonts


RUN cp /usr/share/color/icc/sRGB.icc /app/assets/srgb.icc

RUN fc-cache -f -v

RUN chown -R appuser:appuser /app

USER appuser


EXPOSE 8080

ENTRYPOINT ["/usr/bin/dumb-init", "--"]


CMD ["/app/pdfsynth"]
