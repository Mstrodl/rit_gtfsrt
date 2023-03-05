FROM ghcr.io/rust-lang/rust:nightly-bullseye@sha256:2550ba6cc2d72faa1465ca3b92f442b620580be75269b1b2e71e0c8d6058c7f9

WORKDIR /app
COPY . .

RUN cargo install --config "registries.crates-io.protocol='sparse'" --path . && \
  mkdir /app/http-cacache && \
  chgrp root /app/http-cacache && \
  chmod g+w /app/http-cacache

CMD ["rit_gtfsrt"]
