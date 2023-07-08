#!/bin/bash -e
cargo install --no-default-features --features sqlite sqlx-cli@^0.7
export DATABASE_URL=sqlite:metadata.db 
sqlx database create
sqlx migrate run
cargo sqlx prepare -- --lib "$@"