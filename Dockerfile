# ============================================
# Stage 1: Build das dependências (cacheado)
# ============================================
FROM rust:1.83-bookworm as deps

WORKDIR /app

# Instalar dependências de build necessárias
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copiar apenas arquivos de manifesto para cachear dependências
COPY Cargo.toml Cargo.lock* ./

# Criar src dummy para compilar dependências
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Compilar apenas dependências (será cacheado se Cargo.toml não mudar)
RUN cargo build --release && rm -rf src

# ============================================
# Stage 2: Build do código da aplicação
# ============================================
FROM deps as builder

# Copiar código fonte real
COPY src ./src
COPY assets ./assets
COPY fonts ./fonts

# Tocar no main.rs para forçar recompilação do nosso código (não das deps)
RUN touch src/main.rs

# Build final (rápido pois deps já estão compiladas)
RUN cargo build --release

# ============================================
# Stage 3: Imagem de produção mínima
# ============================================
FROM debian:bookworm-slim

# Instalar runtime dependencies
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

# Criar usuário não-root
RUN useradd -m -u 1000 -U appuser

WORKDIR /app

# Copiar binário compilado
COPY --from=builder /app/target/release/pdfsynth /app/pdfsynth

# Copiar assets
COPY --from=builder /app/assets /app/assets
COPY --from=builder /app/fonts /app/fonts

# Copiar ICC profile do sistema
RUN cp /usr/share/color/icc/sRGB.icc /app/assets/srgb.icc

# Atualizar cache de fontes
RUN fc-cache -f -v

# Ajustar permissões
RUN chown -R appuser:appuser /app

USER appuser

EXPOSE 8080

ENTRYPOINT ["/usr/bin/dumb-init", "--"]
CMD ["/app/pdfsynth"]
