#!/bin/sh

export DATABASE_URL="sqlite:$1"
/app/sqlx database create
/app/sqlx migrate run
exec /app/rugs_metadata_server "--database=$1"