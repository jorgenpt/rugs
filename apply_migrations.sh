#!/bin/bash -e
cargo install sqlx-cli
export DATABASE_URL=sqlite:metadata.db 
sqlx database create
sqlx migrate run
cargo sqlx prepare -- --bin rugs_metadata_server