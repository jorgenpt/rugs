use anyhow::{Context, Result};
use axum::{
    http::{self, Request, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
    Extension, Router,
};
use base64::prelude::*;
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
            .and_then(|authorization_b64| BASE64_STANDARD.decode(authorization_b64).ok())
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
        .route("/latest", get(latest_index))
        .route("/event", get(event_index))
        .route("/comment", get(comment_index))
        .route("/issues", get(issue_index))
        .route("/metadata", get(metadata_index).post(metadata_submit))
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
    use rugs::models::CreateBadge;
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
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .with_context(|| "Could not open in-memory sqlite db")?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(pool)
    }

    /// Test the basic /health API
    #[tokio::test]
    async fn health() -> Result<()> {
        let mut app = app(config(), pool().await?);

        let response = app
            .ready()
            .await?
            .call(Request::builder().uri("/health").body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .ready()
            .await?
            .call(Request::builder().uri("/test/health").body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        Ok(())
    }

    /// Test the basic /health API with a `request_root` configured
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
            .call(Request::builder().uri("/health").body(Body::empty())?)
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .ready()
            .await?
            .call(Request::builder().uri("/test/health").body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }

    /// Helper to format an `Authorization:` header for HTTP Basic Auth requests
    fn authorization_header(token: &str) -> String {
        format!("Basic {}", BASE64_STANDARD.encode(token))
    }

    /// Helper to create HTTP requests
    fn request_builder(
        uri: &str,
        method: &str,
        authorization: Option<String>,
    ) -> http::request::Builder {
        let builder = Request::builder()
            .uri(uri)
            .method(method)
            .header(http::header::CONTENT_TYPE, "application/json");

        if let Some(authorization) = authorization {
            builder.header(http::header::AUTHORIZATION, authorization)
        } else {
            builder
        }
    }

    /// Test that we require auth on all the common user routes
    #[tokio::test]
    async fn user_auth_required() -> Result<()> {
        let paths = vec![
            "/api/latest",
            "/api/event",
            "/api/comment",
            "/api/issues",
            "/api/metadata",
        ];

        // First test without any credentials
        let requests_without_auth = paths
            .iter()
            .map(|path| request_builder(path, "GET", None).body(Body::empty()));
        // First test with bad credentials
        let requests_with_bad_auth = paths.iter().map(|path| {
            request_builder(path, "GET", Some(authorization_header("user:wrong")))
                .body(Body::empty())
        });

        let requests = requests_without_auth
            .chain(requests_with_bad_auth)
            .collect::<Vec<_>>();

        let mut app = app(config(), pool().await?);

        for request in requests {
            let response = app.ready().await?.call(request?).await?;
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "body: {:?}",
                hyper::body::to_bytes(response.into_body()).await?
            );
        }
        Ok(())
    }

    /// Helper to create a basic badge request
    fn simple_create_request() -> CreateBadge {
        CreateBadge {
            change_number: 1,
            url: String::from("http://test.com"),
            build_type: String::from("Editor"),
            result: rugs::models::BadgeResult::Starting,
            project: String::from("//depot/stream/proj"),
        }
    }

    /// Test that we allow requests for user routes when the credentials are correct
    #[tokio::test]
    async fn user_auth_works() -> Result<()> {
        let app = app(config(), pool().await?);

        let create_request = simple_create_request();
        let body = serde_json::to_vec(&create_request)?;
        let response = app
            .oneshot(
                request_builder(
                    "/api/latest?project=//depot/stream/proj",
                    "GET",
                    Some(authorization_header(USER_AUTH)),
                )
                .body(Body::from(body))?,
            )
            .await?;

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "body: {:?}",
            hyper::body::to_bytes(response.into_body()).await?
        );
        Ok(())
    }

    /// Test that we require auth for the CI routes
    #[tokio::test]
    async fn ci_auth_required() -> Result<()> {
        let mut app = app(config(), pool().await?);

        let create_request = simple_create_request();

        // First test without any credentials
        let response = app
            .ready()
            .await?
            .call(
                request_builder("/api/build", "POST", None)
                    .body(Body::from(serde_json::to_vec(&create_request)?))?,
            )
            .await?;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "body: {:?}",
            hyper::body::to_bytes(response.into_body()).await?
        );

        // Then test with bogus credentials
        let response = app
            .ready()
            .await?
            .call(
                request_builder(
                    "/api/build",
                    "POST",
                    Some(authorization_header("ci:invalid")),
                )
                .body(Body::from(serde_json::to_vec(&create_request)?))?,
            )
            .await?;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "body: {:?}",
            hyper::body::to_bytes(response.into_body()).await?
        );
        Ok(())
    }

    /// Test that we allow requests for CI routes when the credentials are correct
    #[tokio::test]
    async fn ci_auth_works() -> Result<()> {
        let app = app(config(), pool().await?);

        let create_request = simple_create_request();
        let body = serde_json::to_vec(&create_request)?;
        let response = app
            .oneshot(
                request_builder("/api/build", "POST", Some(authorization_header(CI_AUTH)))
                    .body(Body::from(body))?,
            )
            .await?;

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "body: {:?}",
            hyper::body::to_bytes(response.into_body()).await?
        );
        Ok(())
    }

    /// Test that we can submit build badges and then read them back
    #[tokio::test]
    async fn metadata_integration() -> Result<()> {
        let mut app = app(config(), pool().await?);

        let creates = [
            CreateBadge {
                change_number: 1,
                url: String::from("http://test.com"),
                build_type: String::from("Editor"),
                result: rugs::models::BadgeResult::Starting,
                project: String::from("//depot/stream/proj"),
            },
            CreateBadge {
                change_number: 1,
                url: String::from("http://test.com"),
                build_type: String::from("Standalone"),
                result: rugs::models::BadgeResult::Starting,
                project: String::from("//depot/stream/proj"),
            },
            CreateBadge {
                change_number: 1,
                url: String::from("http://test.com"),
                build_type: String::from("Editor"),
                result: rugs::models::BadgeResult::Success,
                project: String::from("//depot/stream/proj"),
            },
        ];

        for create in creates {
            let body = serde_json::to_vec(&create)?;

            let response = app
                .ready()
                .await?
                .call(
                    request_builder("/api/build", "POST", Some(authorization_header(CI_AUTH)))
                        .body(Body::from(body))?,
                )
                .await?;

            assert_eq!(response.status(), StatusCode::OK);
        }

        let response = app
            .ready()
            .await?
            .call(
                request_builder(
                    "/api/metadata?stream=//depot/stream&project=proj&sequence=0&minchange=0",
                    "GET",
                    Some(authorization_header(USER_AUTH)),
                )
                .body(Body::empty())?,
            )
            .await?;
        let status = response.status();
        let body = hyper::body::to_bytes(response.into_body()).await?;
        assert_eq!(status, StatusCode::OK, "body: {:?}", body);

        // TODO: Deserialize and verify the response data

        Ok(())
    }
}
