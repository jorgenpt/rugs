use anyhow::anyhow;
use axum::{extract::Query, http::StatusCode, response::IntoResponse, Extension, Json};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use crate::{error::AppError, models::*};

#[derive(Debug, Default)]
pub struct Metrics {
    pub latest_requests: AtomicU64,
    pub build_create_requests: AtomicU64,
    pub metadata_index_requests: AtomicU64,
    pub metadata_submit_requests: AtomicU64,
}

fn find_starting_at(haystack: &str, needle: char, starting_index: usize) -> Option<usize> {
    if let Some(slice) = haystack.get(starting_index..) {
        slice.find(needle).map(|i| i + starting_index)
    } else {
        None
    }
}

/// Take a //depot/stream/project path and try to split it into `//depot/stream` and `project`
fn split_project_path(project_path: &str) -> Option<(String, String)> {
    if !project_path.starts_with("//") {
        return None;
    }

    if let Some(stream_name_index) = find_starting_at(project_path, '/', 2) {
        if let Some(project_index) = find_starting_at(project_path, '/', stream_name_index + 1) {
            if project_path.len() > project_index + 1 {
                Some((
                    normalize_stream(&project_path[0..project_index]),
                    normalize_project_name(&project_path[project_index + 1..]),
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

fn normalize_stream(stream: &str) -> String {
    let stream = stream.strip_suffix('/').unwrap_or(stream);
    stream.to_lowercase()
}

fn normalize_project_name(project_name: &str) -> String {
    project_name.to_lowercase()
}

async fn get_project(
    pool: &SqlitePool,
    stream: &str,
    project_name: &str,
) -> Result<Option<i64>, AppError> {
    let (stream, project_name) = (stream.to_lowercase(), project_name.to_lowercase());

    let project_id = sqlx::query_scalar!(
        "SELECT project_id FROM projects WHERE stream = ? AND project = ? LIMIT 1",
        stream,
        project_name
    )
    .fetch_optional(pool)
    .await?;

    Ok(project_id)
}

async fn get_or_add_project(
    pool: &SqlitePool,
    stream: &str,
    project_name: &str,
) -> Result<i64, AppError> {
    let (stream, project_name) = (stream.to_lowercase(), project_name.to_lowercase());

    let project_id = sqlx::query_scalar!(
        "SELECT project_id FROM projects WHERE stream = ? AND project = ? LIMIT 1",
        stream,
        project_name
    )
    .fetch_optional(pool)
    .await?;

    if let Some(project_id) = project_id {
        Ok(project_id)
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
        pub latest_requests: u64,
        pub build_index_requests: u64,
        pub build_create_requests: u64,
        pub metadata_index_requests: u64,
        pub metadata_submit_requests: u64,
    }

    Json(MetricsResponse {
        latest_requests: metrics.latest_requests.load(Ordering::Relaxed),
        build_index_requests: 0,
        build_create_requests: metrics.build_create_requests.load(Ordering::Relaxed),
        metadata_index_requests: metrics.metadata_index_requests.load(Ordering::Relaxed),
        metadata_submit_requests: metrics.metadata_submit_requests.load(Ordering::Relaxed),
    })
}

#[derive(Debug, Deserialize)]
pub struct LatestParams {
    project: String,
}

pub async fn latest_index(
    Extension(pool): Extension<SqlitePool>,
    Extension(metrics): Extension<Arc<Metrics>>,
    Extension(sequence_lock): Extension<Arc<RwLock<()>>>,
    params: Query<LatestParams>,
) -> Result<impl IntoResponse, AppError> {
    metrics.latest_requests.fetch_add(1, Ordering::Relaxed);

    let (stream, project_name) = split_project_path(&params.project).ok_or_else(|| {
        anyhow!(
            "Invalid project name format {}, should be Perforce stream path to directory",
            params.project
        )
    })?;

    let project_id = get_project(&pool, &stream, &project_name).await?;

    let _read_lock = sequence_lock.read().await;

    let (last_build_id, last_event_id) = if let Some(project_id) = project_id {
        let badge_sequence = sqlx::query_scalar!(
            "SELECT sequence FROM badges WHERE project_id = ? ORDER BY sequence DESC LIMIT 1",
            project_id
        )
        .fetch_optional(&pool)
        .await?;

        let event_sequence = sqlx::query_scalar!(
            "SELECT sequence FROM user_events WHERE project_id = ? ORDER BY sequence DESC LIMIT 1",
            project_id
        )
        .fetch_optional(&pool)
        .await?;
        (
            badge_sequence.unwrap_or_default(),
            event_sequence.unwrap_or_default(),
        )
    } else {
        (0, 0)
    };

    let response = LatestResponseV1 {
        version: Some(2),
        last_build_id,
        last_comment_id: 0,
        last_event_id,
    };
    Ok((StatusCode::OK, Json(response)))
}

/// Handler for POST /api/build, creates a new badge with the given info
pub async fn build_create(
    Extension(pool): Extension<SqlitePool>,
    Extension(metrics): Extension<Arc<Metrics>>,
    Extension(sequence_lock): Extension<Arc<RwLock<()>>>,
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
    let _write_lock = sequence_lock.write().await;

    let project_id = get_or_add_project(&pool, &stream, &project).await?;
    let added_at = chrono::Utc::now();
    let sequence_number = added_at.timestamp_micros();
    let result = badge.result as u8;
    let query = sqlx::query!(
        "INSERT INTO badges (sequence, change_number, added_at, build_type, result, url, project_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
        sequence_number,
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
    Extension(sequence_lock): Extension<Arc<RwLock<()>>>,
    params: Query<MetadataIndexParams>,
) -> Result<impl IntoResponse, AppError> {
    metrics
        .metadata_index_requests
        .fetch_add(1, Ordering::Relaxed);

    let stream = normalize_stream(&params.stream);
    let project = params
        .project
        .to_owned()
        .map(|p| normalize_project_name(&p));

    let project_query_string = format!(
        "SELECT project_id, project FROM projects WHERE stream = ? {}",
        params
            .project
            .is_some()
            .then_some("AND project = ?")
            .unwrap_or_default()
    );

    #[derive(sqlx::FromRow)]
    struct Project {
        project_id: i64,
        project: String,
    }

    let mut project_query =
        sqlx::query_as::<sqlx::Sqlite, Project>(&project_query_string).bind(&stream);
    if let Some(project) = project {
        project_query = project_query.bind(project);
    }

    let projects = project_query.fetch_all(&pool).await?;

    let mut response = GetMetadataListResponseV2 {
        sequence_number: 0,
        items: Vec::new(),
    };

    let _read_lock = sequence_lock.read().await;

    for project in projects {
        let project_path = format!("{}/{}", stream, project.project);

        let mut filters = Vec::new();
        if params.sequence.is_some() {
            filters.push("sequence > ?");
        }

        filters.push("change_number >= ?");
        if params.maxchange.is_some() {
            filters.push("change_number <= ?")
        }

        // We intentionally order these by sequence (from old to new). We don't send the ID, so to manage newness the order here matters.
        // (We could also only send the most recent badge for each (change_number, build_result) pair, but the client will take care
        // of figuring out which the most recent is if we order them right.)
        let badge_query_string = format!(
            "SELECT * FROM badges WHERE project_id = ? AND {} ORDER BY sequence ASC",
            filters.join(" AND "),
        );
        let mut badge_query =
            sqlx::query_as::<sqlx::Sqlite, Badge>(&badge_query_string).bind(project.project_id);

        if let Some(sequence) = params.sequence {
            badge_query = badge_query.bind(sequence);
        }

        badge_query = badge_query.bind(params.minchange);
        if let Some(maxchange) = params.maxchange {
            badge_query = badge_query.bind(maxchange);
        }

        let badges = badge_query.fetch_all(&pool).await?;

        let mut changelists = HashMap::<i64, GetMetadataResponseV2>::new();

        for badge in badges {
            response.sequence_number = response.sequence_number.max(badge.sequence);

            let cl_badges =
                changelists
                    .entry(badge.change_number)
                    .or_insert_with(|| GetMetadataResponseV2 {
                        project: project_path.to_owned(),
                        change: badge.change_number,
                        users: Vec::new(),
                        badges: Vec::new(),
                    });
            cl_badges.badges.push(GetBadgeDataResponseV2 {
                name: badge.build_type,
                url: badge.url,
                state: badge.result,
            });
        }

        // We intentionally order these by sequence (from old to new). We don't send the ID, so to manage newness the order here matters.
        // (We could also only send the most recent badge for each (change_number, build_result) pair, but the client will take care
        // of figuring out which the most recent is if we order them right.)
        let user_event_query_string = format!(
            "SELECT * FROM user_events WHERE project_id = ? AND {} ORDER BY sequence ASC",
            filters.join(" AND "),
        );
        let mut user_event_query =
            sqlx::query_as::<sqlx::Sqlite, UserEvent>(&user_event_query_string)
                .bind(project.project_id);

        if let Some(sequence) = params.sequence {
            user_event_query = user_event_query.bind(sequence);
        }

        user_event_query = user_event_query.bind(params.minchange);
        if let Some(maxchange) = params.maxchange {
            user_event_query = user_event_query.bind(maxchange);
        }

        let user_events = user_event_query.fetch_all(&pool).await?;

        for user_event in user_events {
            response.sequence_number = response.sequence_number.max(user_event.sequence);

            let cl_badges = changelists
                .entry(user_event.change_number)
                .or_insert_with(|| GetMetadataResponseV2 {
                    project: project_path.to_owned(),
                    change: user_event.change_number,
                    users: Vec::new(),
                    badges: Vec::new(),
                });

            cl_badges.users.push(GetUserDataResponseV2 {
                user: user_event.user_name,
                sync_time: user_event.synced_at.map(|t| t.timestamp_micros() * 10),
                vote: user_event.vote,
                comment: user_event.comment,
                investigating: user_event.investigating,
                starred: user_event.starred,
            });
        }

        // Doesn't look like ordering should matter, so don't bother sorting or anything
        response.items.extend(changelists.into_values().into_iter());
    }

    debug!("GET /metadata response: {:?}", response);

    Ok(Json(response))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateMetadataRequestV2 {
    change: i64,
    // This is technically a `string?` in C#, but required by the API unless we're submitting badges
    stream: String,
    project: Option<String>,
    // This is technically a `string?` in C#, but required by the API unless we're submitting badges
    user_name: String,
    synced: Option<bool>,
    vote: Option<UgsUserVote>,
    investigating: Option<bool>,
    starred: Option<bool>,
    comment: Option<String>,
}

pub async fn metadata_submit(
    Extension(pool): Extension<SqlitePool>,
    Extension(metrics): Extension<Arc<Metrics>>,
    Extension(sequence_lock): Extension<Arc<RwLock<()>>>,
    Json(params): Json<UpdateMetadataRequestV2>,
) -> Result<impl IntoResponse, AppError> {
    metrics
        .metadata_submit_requests
        .fetch_add(1, Ordering::Relaxed);

    let _write_lock = sequence_lock.write().await;
    let now = chrono::Utc::now();
    let sequence_number = now.timestamp_micros();

    let stream = normalize_stream(&params.stream);
    let project_name = params
        .project
        .map(|p| normalize_project_name(&p))
        .unwrap_or_default();
    let project_id = get_or_add_project(&pool, &stream, &project_name).await?;
    let existing_event_query_string =
        "SELECT * FROM user_events WHERE project_id = ? AND user_name = ? AND change_number = ?";
    let existing_event_query =
        sqlx::query_as::<sqlx::Sqlite, UserEvent>(&existing_event_query_string)
            .bind(project_id)
            .bind(&params.user_name)
            .bind(params.change);
    let user_event = existing_event_query.fetch_optional(&pool).await?;

    let needs_insert = user_event.is_none();

    let mut user_event = user_event.unwrap_or_else(UserEvent::default);
    if params.synced.unwrap_or_default() {
        user_event.synced_at = Some(now);
    }

    user_event.vote = params.vote.or(user_event.vote);
    user_event.investigating = params.investigating.or(user_event.investigating);
    user_event.starred = params.starred.or(user_event.starred);
    user_event.comment = params.comment.or(user_event.comment);

    if needs_insert {
        sqlx::query!(
            "INSERT INTO user_events (project_id, change_number, user_name, sequence, updated_at, synced_at, vote, investigating, starred, comment) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            project_id,
            params.change,
            params.user_name,
            sequence_number,
            now,
            user_event.synced_at,
            user_event.vote,
            user_event.investigating,
            user_event.starred,
            user_event.comment,
        ).execute(&pool).await?;
    } else {
        sqlx::query!(
            "UPDATE user_events SET sequence = ?, updated_at = ?, synced_at = ?, vote = ?, investigating = ?, starred = ?, comment = ? WHERE id = ?",
            sequence_number,
            now,
            user_event.synced_at,
            user_event.vote,
            user_event.investigating,
            user_event.starred,
            user_event.comment,
            user_event.id,
        ).execute(&pool).await?;
    }

    Ok((StatusCode::OK, ""))
}
