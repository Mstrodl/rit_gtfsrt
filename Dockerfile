FROM docker.io/rust:1.70-bookworm

WORKDIR /app
COPY . .

RUN cargo install --config "registries.crates-io.protocol='sparse'" --path . && \
  mkdir /app/http-cacache && \
  chgrp root /app/http-cacache && \
  chmod g+w /app/http-cacache

CMD ["rit_gtfsrt"]
