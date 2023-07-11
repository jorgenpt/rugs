# Build our rust application using rust's cross-compilation on our current host platform
# Rust cross-compilation is much faster than emulating a Docker container
FROM --platform=$BUILDPLATFORM rust:1.70 as builder

ARG BUILDPLATFORM
ARG TARGETPLATFORM

# Make sure we don't try to access the database during build
ENV SQLX_OFFLINE=true
WORKDIR /build

# Make sure we've configured rustc & cargo for the correct target, and installed any other
# dependencies.
COPY ./docker/cross_build_setup/${BUILDPLATFORM}/${TARGETPLATFORM}.sh cross_build_setup.sh
RUN ./cross_build_setup.sh

# Create a layer with just the sqlx-cli
RUN cargo install --no-default-features --features sqlite sqlx-cli@^0.7

# Then create a layer that is only invalidated if the dependencies change
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src/bin/only_dependencies.rs src/bin/only_dependencies.rs
RUN cargo build --bin=only_dependencies --release

# Then finally build a layer that is invalidated if any of the code is changed
COPY ./.sqlx ./.sqlx
COPY ./src ./src
RUN cargo build --bins --release

# Create the data directory so we have somewhere to write transient data if there's no mount
RUN mkdir -p data

FROM gcr.io/distroless/cc as service

COPY --from=busybox:stable-uclibc /bin/sh /bin/

USER nonroot:nonroot
WORKDIR /app

# And create layers that depend on the scripts & migrations
COPY ./docker/migrate_and_run.sh migrate_and_run.sh
COPY migrations migrations

# Then create layers that depends on the build output
COPY --from=builder --chown=nonroot:nonroot /build/data /data
COPY --from=builder /usr/local/cargo/bin/sqlx sqlx
COPY --from=builder /build/current_target/release/rugs_metadata_server rugs_metadata_server

LABEL org.opencontainers.image.description="An efficient, easy-to-deploy alternative to Epic's official metadata server for Unreal Game Sync."
ENV RUST_LOG=info
ENTRYPOINT ["/app/migrate_and_run.sh", "/data/metadata.db"]
