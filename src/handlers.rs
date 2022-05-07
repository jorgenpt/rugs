use anyhow::anyhow;
use axum::{extract::Query, http::StatusCode, response::IntoResponse, Extension, Json};
use num_traits::FromPrimitive;
use serde::Deserialize;
use sqlx::SqlitePool;
use tracing::{error, info};

use std::num::NonZeroI64;

use crate::{error::AppError, models::*};

async fn get_or_add_project(pool: &SqlitePool, project_name: &str) -> Result<i64, AppError> {
    let record = sqlx::query!(
        "SELECT project_id FROM projects WHERE project = ? LIMIT 1",
        project_name
    )
    .fetch_optional(pool)
    .await?;

    if let Some(record) = record {
        Ok(record.project_id)
    } else {
        // TODO: Thread safe
        Ok(
            sqlx::query!("INSERT INTO projects (project) VALUES (?)", project_name)
                .execute(pool)
                .await?
                .last_insert_rowid(),
        )
    }
}

#[derive(Debug, Deserialize)]
pub struct LatestParams {
    project: String,
}

pub async fn latest(
    Extension(pool): Extension<SqlitePool>,
    params: Query<LatestParams>,
) -> Result<impl IntoResponse, AppError> {
    let row = sqlx::query!(
        "SELECT id FROM badges INNER JOIN projects USING(project_id) WHERE project = ? ORDER BY id DESC LIMIT 1",
        params.project
    )
    .fetch_optional(&pool)
    .await?;

    let response = LatestResponseV1 {
        last_build_id: row.map_or(0, |row| row.id),
        last_comment_id: 0,
        last_event_id: 0,
    };
    Ok((StatusCode::OK, Json(response)))
}

#[derive(Debug, Deserialize)]
pub struct BadgesParams {
    project: String,
    lastbuildid: i64,
}

/// Handler for GET /build?project=foo&lastbuildid=42, returns a filtered list of badges
pub async fn build_index(
    Extension(pool): Extension<SqlitePool>,
    params: Query<BadgesParams>,
) -> Result<impl IntoResponse, AppError> {
    info!(
        "project: {}, build id: {}",
        params.project, params.lastbuildid
    );
    let response = sqlx::query!(
        "SELECT * FROM badges INNER JOIN projects USING(project_id) WHERE id > ? AND project = ? ORDER BY id ASC",
        params.lastbuildid,
        params.project
    )
    .map(|row| -> Result<Badge, AppError> {
        Ok(Badge {
            id: NonZeroI64::new(row.id),
            change_number: row.change_number,
            added_at: chrono::DateTime::<chrono::Utc>::from_utc(row.added_at, chrono::Utc),
            build_type: row.build_type,
            result: BuildDataResult::from_i64(row.result).ok_or_else(|| anyhow!("Invalid build data result in db for {}", row.id))?,
            url: row.url,
            project: row.project,
            archive_path: row.archive_path,
        })
    })
    .fetch_all(&pool)
    .await?;

    let (badges, errors): (Vec<_>, Vec<_>) = response.into_iter().partition(|r| r.is_ok());

    for error in errors {
        error!("bad badge in database: {}", error.unwrap_err().0);
    }

    let badges: Vec<Badge> = badges.into_iter().map(|badge| badge.unwrap()).collect();

    Ok((StatusCode::OK, Json(badges)))
}

/// Handler for POST /api/build, creates a new badge with the given info
pub async fn build_create(
    Extension(pool): Extension<SqlitePool>,
    Json(badge): Json<CreateBadge>,
) -> Result<impl IntoResponse, AppError> {
    let project_id = get_or_add_project(&pool, &badge.project).await?;
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
    query.execute(&pool).await?;

    Ok((StatusCode::OK, ""))
}

/// Handler for GET /event, currently just a placeholder empty response to
/// prevent error logging in UGS.
pub async fn event_index() -> impl IntoResponse {
    let response: [&str; 0] = [];
    // Unimplemented for now
    (StatusCode::OK, Json(response))
}

/// Handler for GET /comment, currently just a placeholder empty response to
/// prevent error logging in UGS.
pub async fn comment_index() -> impl IntoResponse {
    let response: [&str; 0] = [];
    // Unimplemented for now
    (StatusCode::OK, Json(response))
}

/// Handler for GET /issues, currently just a placeholder empty response to
/// prevent error logging in UGS.
pub async fn issue_index() -> impl IntoResponse {
    let response: [&str; 0] = [];
    // Unimplemented for now
    (StatusCode::OK, Json(response))
}
