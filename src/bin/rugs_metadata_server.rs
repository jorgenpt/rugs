use anyhow::{Context, Result};
use axum::{
    http::{self, Request, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
    Extension, Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::error;

use std::{fmt::Display, fs::File, io::BufReader, net::SocketAddr, path::Path, sync::Arc};

use rugs::handlers::*;

/// A simple authenticated metadata server for UGS
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Config file to load settings from
    #[clap(long, default_value = "config.json")]
    config: String,

    /// Path to the sqlite database to use for persistence (you can
    /// use `:memory:` to not persist))
    #[clap(long, default_value = "metadata.db")]
    database: String,
}

fn default_request_root() -> String {
    "/".to_owned()
}

/// Configuration loaded from disk
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Config {
    /// The auth token required for user-facing operations (reading badges, leaving comments & feedback)
    pub user_auth: String,
    /// The auth token required for CI-facing operations (writing badges)
    pub ci_auth: String,
    /// The prefix we expect for any request (e.g. "/ugs" means we look for "/ugs/api/build")
    #[serde(default = "default_request_root")]
    pub request_root: String,
}

/// Parse a `Config` from JSON at the given path
fn read_config_from_file<P: AsRef<Path> + Display>(path: P) -> Result<Config> {
    let file = File::open(&path).with_context(|| format!("Failed to read config from {}", path))?;
    let reader = BufReader::new(file);

    let config = serde_json::from_reader(reader)
        .with_context(|| format!("Failed to parse config in {}", path))?;
    Ok(config)
}

/// Require a Basic Auth header that matches `required_auth`, or deny the request. If `required_auth`
/// is empty, allow any request.
async fn auth<B>(req: Request<B>, next: Next<B>, required_auth: String) -> impl IntoResponse {
    if required_auth.is_empty() {
        Ok(next.run(req).await)
    } else if let Some(auth_header) = req.headers().get(http::header::AUTHORIZATION) {
        let authorization = auth_header
            .to_str()
            .ok()
            .and_then(|header| header.strip_prefix("Basic "))
            .and_then(|authorization_b64| base64::decode(authorization_b64).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok());

        match authorization {
            Some(authorization) => {
                if authorization == required_auth {
                    Ok(next.run(req).await)
                } else {
                    error!("Invalid token in Authorization header, denying");
                    Err(StatusCode::UNAUTHORIZED)
                }
            }
            None => {
                error!("Bogus Authorization header {:?}, denying", auth_header);
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Just returns a 200.
pub async fn health() {}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let config = read_config_from_file(args.config)?;

    let pool = SqlitePool::connect(&format!("sqlite:{}", args.database))
        .await
        .with_context(|| format!("Could not open database at {}", args.database))?;

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app(config, pool).into_make_service())
        .await?;

    Ok(())
}

fn app(config: Config, pool: SqlitePool) -> Router {
    // Configure routes that require the `user_auth` token (these are expected to come from
    // the UGS client).
    let user_routes = Router::new()
        .route("/latest", get(latest))
        .route("/build", get(build_index))
        .route("/event", get(event_index))
        .route("/comment", get(comment_index))
        .route("/issues", get(issue_index))
        .layer(middleware::from_fn(move |req, next| {
            auth(req, next, config.user_auth.clone())
        }));

    // Configure routes that require the `ci_auth` token (these are expected to come from your
    // CI service, e.g. PostBadgeStatus.exe)
    let ci_routes = Router::new()
        .route("/build", post(build_create))
        // Back compat with old PostBadgeStatus.exe which uses the wrong case
        .route("/Build", post(build_create))
        .route("/rugs_metrics", get(metrics_index))
        .layer(middleware::from_fn(move |req, next| {
            auth(req, next, config.ci_auth.clone())
        }));

    let app = Router::new().nest(
        &config.request_root,
        Router::new()
            .nest("/api", Router::new().merge(user_routes).merge(ci_routes))
            .route("/health", get(health)),
    );

    // We expose the basic `health` endpoint under both `/health` and `/<request_root>/health` if the
    // root isn't already `/`
    let app = if config.request_root != "/" {
        app.route("/health", get(health))
    } else {
        app
    };

    let metrics = Arc::new(Metrics::default());

    app.layer(
        ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())
            .layer(Extension(pool))
            .layer(Extension(metrics)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::{Service, ServiceExt};

    const CI_AUTH: &str = "ci:ci";
    const USER_AUTH: &str = "user:user";

    fn config() -> Config {
        Config {
            user_auth: USER_AUTH.to_string(),
            ci_auth: CI_AUTH.to_string(),
            request_root: "/".to_string(),
        }
    }

    async fn pool() -> Result<SqlitePool> {
        SqlitePool::connect("sqlite::memory:")
            .await
            .with_context(|| "Could not open in-memory sqlite db")
    }

    #[tokio::test]
    async fn health() -> Result<()> {
        let mut app = app(config(), pool().await?);

        let response = app
            .ready()
            .await?
            .call(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .ready()
            .await?
            .call(
                Request::builder()
                    .uri("/test/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await?;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        Ok(())
    }

    #[tokio::test]
    async fn health_with_root() -> Result<()> {
        let cfg = Config {
            request_root: String::from("/test"),
            ..config()
        };
        let mut app = app(cfg, pool().await?);

        let response = app
            .ready()
            .await?
            .call(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .ready()
            .await?
            .call(
                Request::builder()
                    .uri("/test/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }
}
