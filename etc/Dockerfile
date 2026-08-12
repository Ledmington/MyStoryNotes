# Builds and runs MyStoryNotes in a container. The app is a windowed GUI program, so the
# container needs access to a display server on the host — see the "Running in Docker" section
# of the README for how to launch it with X11 forwarding.
#
# Build from the repository root (not from etc/), since the build needs the whole source tree:
#   docker build -t MyStoryNotes -f etc/Dockerfile .

FROM rust:1.97.0-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libgtk-3-dev \
    libxkbcommon-dev \
    libx11-dev \
    libxi-dev \
    libxrandr-dev \
    libxcursor-dev \
    libxinerama-dev \
    libwayland-dev \
    libgl1-mesa-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libgtk-3-0 \
    libxkbcommon0 \
    libxkbcommon-x11-0 \
    libx11-6 \
    libxi6 \
    libxrandr2 \
    libxcursor1 \
    libxinerama1 \
    libwayland-client0 \
    libgl1 \
    libegl1 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/MyStoryNotes /usr/local/bin/MyStoryNotes

ENTRYPOINT ["MyStoryNotes"]
