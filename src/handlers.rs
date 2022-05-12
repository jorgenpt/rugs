use anyhow::anyhow;
use axum::{extract::Query, http::StatusCode, response::IntoResponse, Extension, Json};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tracing::{debug, error, info};

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use crate::{error::AppError, models::*};

#[derive(Debug, Default)]
pub struct Metrics {
    pub latest_requests: AtomicU32,
    pub build_index_requests: AtomicU32,
    pub build_create_requests: AtomicU32,
    pub metadata_index_requests: AtomicU32,
}

fn find_starting_at(haystack: &str, needle: char, starting_index: usize) -> Option<usize> {
    if let Some(slice) = haystack.get(starting_index..) {
        slice.find(needle).map(|i| i + starting_index)
    } else {
        None
    }
}

/// Take a //depot/stream/project path and try to split it into `//depot/stream` and `project`
fn split_project_path(project_path: &str) -> Option<(&str, &str)> {
    if !project_path.starts_with("//") {
        return None;
    }

    if let Some(stream_name_index) = find_starting_at(project_path, '/', 2) {
        if let Some(project_index) = find_starting_at(project_path, '/', stream_name_index + 1) {
            if project_path.len() > project_index + 1 {
                Some((
                    &project_path[0..project_index],
                    &project_path[project_index + 1..],
                ))
            } else {
                error!(
                    "Not enough characters after stream name in {}",
                    project_path
                );
                None
            }
        } else {
            error!(
                "Could not find a project name after stream name in {}",
                project_path
            );
            None
        }
    } else {
        error!("Could not find a stream name in {}", project_path);
        None
    }
}

async fn get_or_add_project(
    pool: &SqlitePool,
    stream: &str,
    project_name: &str,
) -> Result<i64, AppError> {
    let record = sqlx::query!(
        "SELECT project_id FROM projects WHERE stream = ? AND project = ? LIMIT 1",
        stream,
        project_name
    )
    .fetch_optional(pool)
    .await?;

    if let Some(record) = record {
        Ok(record.project_id)
    } else {
        info!(
            "Creating new project for stream {}, project name {}",
            stream, project_name
        );

        // TODO: Thread safety
        Ok(sqlx::query!(
            "INSERT INTO projects (stream, project) VALUES (?, ?)",
            stream,
            project_name
        )
        .execute(pool)
        .await?
        .last_insert_rowid())
    }
}

pub async fn metrics_index(Extension(metrics): Extension<Arc<Metrics>>) -> impl IntoResponse {
    #[derive(Serialize)]
    struct MetricsResponse {
        pub latest_requests: u32,
        pub build_index_requests: u32,
        pub build_create_requests: u32,
        pub metadata_index_requests: u32,
    }

    Json(MetricsResponse {
        latest_requests: metrics.latest_requests.load(Ordering::Relaxed),
        build_index_requests: metrics.build_index_requests.load(Ordering::Relaxed),
        build_create_requests: metrics.build_create_requests.load(Ordering::Relaxed),
        metadata_index_requests: metrics.metadata_index_requests.load(Ordering::Relaxed),
    })
}

#[derive(Debug, Deserialize)]
pub struct LatestParams {
    project: String,
}

pub async fn latest_index(
    Extension(pool): Extension<SqlitePool>,
    Extension(metrics): Extension<Arc<Metrics>>,
    params: Query<LatestParams>,
) -> Result<impl IntoResponse, AppError> {
    metrics.latest_requests.fetch_add(1, Ordering::Relaxed);

    let (stream, project) = split_project_path(&params.project).ok_or_else(|| {
        anyhow!(
            "Invalid project name format {}, should be Perforce stream path to directory",
            params.project
        )
    })?;

    let row = sqlx::query!(
        "SELECT id FROM badges INNER JOIN projects USING(project_id) WHERE stream = ? AND project = ? ORDER BY id DESC LIMIT 1",
        stream, project
    )
    .fetch_optional(&pool)
    .await?;

    let response = LatestResponseV1 {
        version: Some(2),
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

/// Handler for GET /build?project=foo&lastbuildid=42, returns a filtered list of badges (Use by v1 API clients)
pub async fn build_index(
    Extension(pool): Extension<SqlitePool>,
    Extension(metrics): Extension<Arc<Metrics>>,
    params: Query<BadgesParams>,
) -> Result<impl IntoResponse, AppError> {
    metrics.build_index_requests.fetch_add(1, Ordering::Relaxed);
    let (stream, project) = split_project_path(&params.project).ok_or_else(|| {
        anyhow!(
            "Invalid project name format {}, should be Perforce stream path to directory",
            params.project
        )
    })?;

    let badges = sqlx::query_as::<sqlx::Sqlite, Badge>(
        "SELECT * FROM badges INNER JOIN projects USING(project_id) WHERE id > ? AND stream = ? AND project = ? ORDER BY id ASC"
    ).bind(
        params.lastbuildid).bind(
        stream).bind(project)
    .fetch_all(&pool)
    .await?;

    debug!("GET /build response: {:?}", badges);

    Ok((StatusCode::OK, Json(badges)))
}

/// Handler for POST /api/build, creates a new badge with the given info
pub async fn build_create(
    Extension(pool): Extension<SqlitePool>,
    Extension(metrics): Extension<Arc<Metrics>>,
    Json(badge): Json<CreateBadge>,
) -> Result<impl IntoResponse, AppError> {
    metrics
        .build_create_requests
        .fetch_add(1, Ordering::Relaxed);

    let (stream, project) = split_project_path(&badge.project).ok_or_else(|| {
        anyhow!(
            "Invalid project name format {}, should be Perforce stream path to directory",
            badge.project
        )
    })?;

    debug!("POST /build request: {:?}", badge);
    let project_id = get_or_add_project(&pool, stream, project).await?;
    let added_at = chrono::Utc::now();
    let result = badge.result as u8;
    let query = sqlx::query!(
        "INSERT INTO badges (change_number, added_at, build_type, result, url, project_id) VALUES (?, ?, ?, ?, ?, ?)",
        badge.change_number,
        added_at,
        badge.build_type,
        result,
        badge.url,
        project_id,
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

#[derive(Debug, Deserialize)]
pub struct MetadataIndexParams {
    stream: String,
    project: Option<String>,
    minchange: i64,
    maxchange: Option<i64>,
    sequence: Option<i64>,
}

/// Handler for GET /metadata (Used by v2 API clients)
pub async fn metadata_index(
    Extension(pool): Extension<SqlitePool>,
    Extension(metrics): Extension<Arc<Metrics>>,
    params: Query<MetadataIndexParams>,
) -> Result<impl IntoResponse, AppError> {
    metrics
        .metadata_index_requests
        .fetch_add(1, Ordering::Relaxed);

    let mut filters = Vec::new();
    if params.sequence.is_some() {
        filters.push("id > ?");
    }

    filters.push("change_number >= ?");
    if params.maxchange.is_some() {
        filters.push("change_number <= ?")
    }

    filters.push("stream = ?");
    if params.project.is_some() {
        filters.push("project = ?");
    }

    let query_string = format!(
        "SELECT * FROM badges INNER JOIN projects USING(project_id) WHERE {} ORDER BY project_id ASC, change_number ASC",
        filters.join(" AND ")
    );

    let mut query = sqlx::query_as::<sqlx::Sqlite, Badge>(&query_string);

    if let Some(sequence) = params.sequence {
        query = query.bind(sequence);
    }

    query = query.bind(params.minchange);
    if let Some(maxchange) = params.maxchange {
        query = query.bind(maxchange);
    }

    query = query.bind(&params.stream);
    if let Some(project) = &params.project {
        query = query.bind(project);
    }

    let badges = query.fetch_all(&pool).await?;

    let mut response = GetMetadataListResponseV2 {
        sequence_number: 0,
        items: Vec::new(),
    };

    for badge in badges {
        response.sequence_number = response.sequence_number.max(badge.id);
        let needs_new_record = !response
            .items
            .last()
            .map_or(false, |r| r.matches(&badge.project, badge.change_number));
        if needs_new_record {
            response.items.push(GetMetadataResponseV2 {
                project: badge.project,
                change: badge.change_number,
                users: Vec::new(),
                badges: Vec::new(),
            });
        }

        response
            .items
            .last_mut()
            .unwrap()
            .badges
            .push(GetBadgeDataResponseV2 {
                name: badge.build_type,
                url: badge.url,
                state: badge.result,
            });
    }

    debug!("GET /metadata response: {:?}", response);

    Ok(Json(response))
}
