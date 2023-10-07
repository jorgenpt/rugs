cargo install --no-default-features --features sqlite sqlx-cli@^^0.7
@if %errorlevel% neq 0 exit /b %errorlevel%

set DATABASE_URL=sqlite:metadata.db 
sqlx database create
@if %errorlevel% neq 0 exit /b %errorlevel%

sqlx migrate run
@if %errorlevel% neq 0 exit /b %errorlevel%

cargo sqlx prepare -- --lib %*
@exit /b %errorlevel%
