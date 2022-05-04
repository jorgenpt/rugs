use axum::{
    body::{Body, Bytes},
    extract::Query,
    http::{Request, Response, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::get,
    Extension, Json, Router,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use tracing::info;

use std::{net::SocketAddr, num::NonZeroI64, sync::Arc};

mod models;
use models::*;

struct Config {}

#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(tracing::Level::DEBUG)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let config = Arc::new(Config {});
    let pool = SqlitePool::connect("sqlite:metadata.db").await.unwrap();
    sqlx::migrate::Migrator::new(std::path::Path::new("./migrations"))
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap();

    // build our application with a route
    let app = Router::new()
        .route("/api/latest", get(latest))
        .route("/api/build", get(badges).post(add_badge))
        .route("/api/Build", get(badges).post(add_badge))
        .route("/api/event", get(events))
        .route("/api/comment", get(comments))
        .route("/api/issues", get(issues))
        .layer(middleware::from_fn(print_request_response))
        .layer(Extension(config))
        .layer(Extension(pool));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

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
        result: BuildDataResult::Success,
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

async fn print_request_response(
    req: Request<Body>,
    next: Next<Body>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (parts, body) = req.into_parts();
    let bytes = buffer_and_print("request", body).await?;
    let req = Request::from_parts(parts, Body::from(bytes));

    let res = next.run(req).await;

    let (parts, body) = res.into_parts();
    let bytes = buffer_and_print("response", body).await?;
    let res = Response::from_parts(parts, Body::from(bytes));

    Ok(res)
}

async fn buffer_and_print<B>(direction: &str, body: B) -> Result<Bytes, (StatusCode, String)>
where
    B: axum::body::HttpBody<Data = Bytes>,
    B::Error: std::fmt::Display,
{
    let bytes = match hyper::body::to_bytes(body).await {
        Ok(bytes) => bytes,
        Err(err) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("failed to read {} body: {}", direction, err),
            ));
        }
    };

    if let Ok(body) = std::str::from_utf8(&bytes) {
        tracing::debug!("{} body = {:?}", direction, body);
    }

    Ok(bytes)
}
