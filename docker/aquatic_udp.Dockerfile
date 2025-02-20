# syntax=docker/dockerfile:1

FROM rust:latest AS builder

WORKDIR /usr/src/aquatic

COPY . .

RUN . ./scripts/env-native-cpu-without-avx-512 && cargo build --release -p aquatic_udp

FROM debian:stable-slim


ENV CONFIG_FILE_CONTENTS "[statistics]\ninterval = 5\nprint_to_stdout = true"
ENV ACCESS_LIST_CONTENTS ""

WORKDIR /etc/aquatic/

COPY --from=builder /usr/src/aquatic/target/release/aquatic_udp /usr/local/bin/aquatic_udp

COPY <<-"EOT" /usr/local/bin/entrypoint.sh
#!/bin/bash
echo -e "$CONFIG_FILE_CONTENTS" > /etc/aquatic/config.toml
echo -e "$ACCESS_LIST_CONTENTS" > /var/lib/aquatic/whitelist
exec /usr/local/bin/aquatic_udp -c /etc/aquatic/config.toml "$@"
EOT

RUN mkdir -p /var/lib/aquatic && \
  touch /var/lib/aquatic/whitelist && \
  chmod 0666 /var/lib/aquatic/whitelist && \
  chmod +x /usr/local/bin/entrypoint.sh



HEALTHCHECK --interval=5s --timeout=3s --retries=3 \
  CMD pidof aquatic_udp || exit 1

## we cd before running to workaround nektos/act behavior which overrides WORKDIR
ENTRYPOINT ["sh", "-c", "cd /etc/aquatic && /usr/local/bin/entrypoint.sh"]
# ENTRYPOINT ["tail", "-f", "/dev/null"] # for debugging