#!/bin/sh

export DATABASE_URL="sqlite:$1"
/app/sqlx database create || exit 1
/app/sqlx migrate run || exit 1
exec /app/rugs_metadata_server "--database=$1"