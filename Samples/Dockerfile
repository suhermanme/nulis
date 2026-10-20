# Nulis sample: Dockerfile
# syntax=docker/dockerfile:1
FROM rust:1.80 AS build
WORKDIR /src
COPY . .

# Stage 0: alpha
FROM debian:bookworm-slim AS alpha
ARG VERSION=0.0.0
ENV APP_NAME="alpha" PORT=8000
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/app /usr/local/bin/alpha
EXPOSE 8000

# Stage 1: beta
FROM debian:bookworm-slim AS beta
ARG VERSION=0.1.0
ENV APP_NAME="beta" PORT=8001
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/app /usr/local/bin/beta
EXPOSE 8001

# Stage 2: gamma
FROM debian:bookworm-slim AS gamma
ARG VERSION=0.2.0
ENV APP_NAME="gamma" PORT=8002
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/app /usr/local/bin/gamma
EXPOSE 8002

# Stage 3: delta
FROM debian:bookworm-slim AS delta
ARG VERSION=0.3.0
ENV APP_NAME="delta" PORT=8003
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/app /usr/local/bin/delta
EXPOSE 8003

# Stage 4: epsilon
FROM debian:bookworm-slim AS epsilon
ARG VERSION=0.4.0
ENV APP_NAME="epsilon" PORT=8004
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/app /usr/local/bin/epsilon
EXPOSE 8004

# Stage 5: zeta
FROM debian:bookworm-slim AS zeta
ARG VERSION=0.5.0
ENV APP_NAME="zeta" PORT=8005
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/app /usr/local/bin/zeta
EXPOSE 8005

# Stage 6: eta
FROM debian:bookworm-slim AS eta
ARG VERSION=0.6.0
ENV APP_NAME="eta" PORT=8006
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/app /usr/local/bin/eta
EXPOSE 8006

# Stage 7: theta
FROM debian:bookworm-slim AS theta
ARG VERSION=0.7.0
ENV APP_NAME="theta" PORT=8007
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/app /usr/local/bin/theta
EXPOSE 8007

# Stage 8: iota
FROM debian:bookworm-slim AS iota
ARG VERSION=0.8.0
ENV APP_NAME="iota" PORT=8008
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/app /usr/local/bin/iota
EXPOSE 8008

# Stage 9: kappa
FROM debian:bookworm-slim AS kappa
ARG VERSION=0.9.0
ENV APP_NAME="kappa" PORT=8009
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/app /usr/local/bin/kappa
EXPOSE 8009
CMD ["/usr/local/bin/alpha", "--port", "8000"]
