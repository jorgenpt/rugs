use axum::{
    extract::Query,
    http::{self, Request, StatusCode, Uri},
    middleware::{self, Next},
    response::{ErrorResponse, IntoResponse},
    routing::{get, post},
    Extension, Json, Router,
};
use num_traits::FromPrimitive;
use serde::Deserialize;
use sqlx::SqlitePool;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

use std::{error::Error, fs::File, io::BufReader, net::SocketAddr, num::NonZeroI64, path::Path};

use rugs::models::*;

fn read_config_from_file<P: AsRef<Path>>(path: P) -> Result<Config, Box<dyn Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let config = serde_json::from_reader(reader)?;
    Ok(config)
}

async fn auth<B>(req: Request<B>, next: Next<B>, required_auth: String) -> impl IntoResponse {
    if required_auth.is_empty() {
        Ok(next.run(req).await)
    } else {
        let auth_header = req.headers().get(http::header::AUTHORIZATION);

        let auth_header = auth_header
            .and_then(|header| header.to_str().ok())
            .and_then(|header| header.strip_prefix("Basic "))
            .and_then(|authorization_b64| base64::decode(authorization_b64).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok());

        match auth_header {
            Some(auth_header) if auth_header == required_auth => Ok(next.run(req).await),
            _ => Err(StatusCode::UNAUTHORIZED),
        }
    }
}

// Currently unused as these types of layers run too late to affect routing
#[allow(dead_code)]
async fn lowercase_uri<B>(mut req: Request<B>, next: Next<B>) -> impl IntoResponse {
    let mut new_uri = Uri::builder();
    if let Some(scheme) = req.uri().scheme() {
        new_uri = new_uri.scheme(scheme.to_owned());
    }
    if let Some(authority) = req.uri().authority() {
        new_uri = new_uri.authority(authority.to_owned());
    }
    if let Some(p_and_q) = req.uri().path_and_query() {
        let new_path_and_query = if let Some(query) = p_and_q.query() {
            p_and_q.path().to_lowercase() + "?" + query
        } else {
            p_and_q.path().to_lowercase()
        };

        new_uri = new_uri.path_and_query(new_path_and_query);
    }
    *req.uri_mut() = new_uri.build().unwrap();
    tracing::debug!("new uri: {:?}", req.uri());
    Ok::<_, ErrorResponse>(next.run(req).await)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = read_config_from_file("config.json").unwrap();

    let pool = SqlitePool::connect("sqlite:metadata.db").await.unwrap();

    let user_routes = Router::new()
        .route("/latest", get(latest))
        .route("/build", get(badges))
        .route("/event", get(events))
        .route("/comment", get(comments))
        .route("/issues", get(issues))
        .layer(middleware::from_fn(move |req, next| {
            auth(req, next, config.user_auth.clone())
        }));

    let admin_routes = Router::new()
        .route("/build", post(add_badge))
        .route("/Build", post(add_badge)) // Back compat with old PostBadgeStatus.exe
        .layer(middleware::from_fn(move |req, next| {
            auth(req, next, config.ci_auth.clone())
        }));

    let app = Router::new().nest(
        &config.request_root,
        Router::new()
            .nest("/api", Router::new().merge(user_routes).merge(admin_routes))
            .route("/health", get(health)),
    );

    let app = if config.request_root != "/" {
        app.route("/health", get(health))
    } else {
        app
    };

    let app = app.layer(
        ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())
            .layer(Extension(pool)),
    );

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// Just returns a 200.
async fn health() {}

#[derive(Debug, Deserialize)]
struct LatestParams {
    project: String,
}

async fn latest(
    Extension(pool): Extension<SqlitePool>,
    params: Query<LatestParams>,
) -> impl IntoResponse {
    let row = sqlx::query!(
        "SELECT id FROM badges INNER JOIN projects USING(project_id) WHERE project = ? ORDER BY id DESC LIMIT 1",
        params.project
    )
    .fetch_optional(&pool)
    .await
    .unwrap();

    let response = LatestResponseV1 {
        last_build_id: row.map_or(0, |row| row.id),
        last_comment_id: 0,
        last_event_id: 0,
    };
    (StatusCode::OK, Json(response))
}

#[derive(Debug, Deserialize)]
struct BadgesParams {
    project: String,
    lastbuildid: i64,
}

async fn badges(
    Extension(pool): Extension<SqlitePool>,
    params: Query<BadgesParams>,
) -> impl IntoResponse {
    info!(
        "project: {}, build id: {}",
        params.project, params.lastbuildid
    );
    let response = sqlx::query!(
        "SELECT * FROM badges INNER JOIN projects USING(project_id) WHERE id > ? AND project = ? ORDER BY id ASC",
        params.lastbuildid,
        params.project
    )
    .map(|row| Badge {
        id: NonZeroI64::new(row.id),
        change_number: row.change_number,
        added_at: chrono::DateTime::<chrono::Utc>::from_utc(row.added_at, chrono::Utc),
        build_type: row.build_type,
        result: BuildDataResult::from_i64(row.result).unwrap(),
        url: row.url,
        project: row.project,
        archive_path: row.archive_path,
    })
    .fetch_all(&pool)
    .await
    .unwrap();

    (StatusCode::OK, Json(response))
}

async fn get_or_add_project(pool: &SqlitePool, project_name: &str) -> i64 {
    let record = sqlx::query!(
        "SELECT project_id FROM projects WHERE project = ? LIMIT 1",
        project_name
    )
    .fetch_optional(pool)
    .await
    .unwrap();

    if let Some(record) = record {
        record.project_id
    } else {
        // TODO: Thread safe
        sqlx::query!("INSERT INTO projects (project) VALUES (?)", project_name)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid()
    }
}

async fn events() -> impl IntoResponse {
    let response: [&str; 0] = [];
    // Unimplemented for now
    (StatusCode::OK, Json(response))
}

async fn comments() -> impl IntoResponse {
    let response: [&str; 0] = [];
    // Unimplemented for now
    (StatusCode::OK, Json(response))
}

async fn issues() -> impl IntoResponse {
    let response: [&str; 0] = [];
    // Unimplemented for now
    (StatusCode::OK, Json(response))
}

async fn add_badge(
    Extension(pool): Extension<SqlitePool>,
    Json(badge): Json<CreateBadge>,
) -> impl IntoResponse {
    let project_id = get_or_add_project(&pool, &badge.project).await;
    let added_at = chrono::Utc::now();
    let result = badge.result as u8;
    let query = sqlx::query!(
        "INSERT INTO badges (change_number, added_at, build_type, result, url, project_id, archive_path) VALUES (?, ?, ?, ?, ?, ?, ?)",
        badge.change_number,
        added_at,
        badge.build_type,
        result,
        badge.url,
        project_id,
        badge.archive_path,
    );
    query.execute(&pool).await.unwrap();

    (StatusCode::OK, "")
}
