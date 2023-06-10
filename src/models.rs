use chrono::{DateTime, Utc};
use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// This maps to `LatestData` in MetadataServer & UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LatestResponseV1 {
    pub version: Option<i64>,
    pub last_event_id: i64,
    pub last_comment_id: i64,
    pub last_build_id: i64,
}

#[derive(
    Clone,
    Copy,
    PartialEq,
    Debug,
    Serialize_repr,
    Deserialize_repr,
    FromPrimitive,
    ToPrimitive,
    sqlx::Type,
)]
#[repr(u8)]
pub enum BadgeResult {
    Starting = 0,
    Failure = 1,
    Warning = 2,
    Success = 3,
    Skipped = 4,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "PascalCase")]
pub struct Badge {
    pub sequence: i64,
    pub change_number: i64,
    pub added_at: DateTime<Utc>,
    pub build_type: String,
    pub result: BadgeResult,
    pub url: String,
}

/// This maps to `BuildData` in MetadataServer, `BadgeData` in UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateBadge {
    pub change_number: i64,
    pub build_type: String,
    pub result: BadgeResult,
    pub url: String,
    pub project: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::Type)]
#[repr(u8)]
pub enum UgsUserVote {
    None = 0,
    CompileSuccess = 1,
    CompileFailure = 2,
    Good = 3,
    Bad = 4,
}

#[derive(Clone, Debug, Default, sqlx::FromRow)]
pub struct UserEvent {
    pub id: i64,
    pub change_number: i64,
    pub user_name: String,
    pub sequence: i64,
    pub updated_at: DateTime<Utc>,
    pub synced_at: Option<DateTime<Utc>>,
    pub vote: Option<UgsUserVote>,
    pub starred: Option<bool>,
    pub investigating: Option<bool>,
    pub comment: Option<String>,
}

/// This maps to `GetUserDataResponseV2` in UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetUserDataResponseV2 {
    pub user: String,
    pub sync_time: Option<i64>,
    pub vote: Option<UgsUserVote>,
    pub comment: Option<String>,
    pub investigating: Option<bool>,
    pub starred: Option<bool>,
}

/// This maps to `GetBadgeDataResponseV2` in UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetBadgeDataResponseV2 {
    pub name: String,
    pub url: String,
    pub state: BadgeResult,
}

/// This maps to `GetMetadataResponseV2` in UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetMetadataResponseV2 {
    pub change: i64,
    pub project: String,
    pub users: Vec<GetUserDataResponseV2>,
    pub badges: Vec<GetBadgeDataResponseV2>,
}

impl GetMetadataResponseV2 {
    pub fn matches(&self, project: &str, change: i64) -> bool {
        self.project == project && self.change == change
    }
}

/// This maps to `GetMetadataListResponseV2` in UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetMetadataListResponseV2 {
    pub sequence_number: i64,
    pub items: Vec<GetMetadataResponseV2>,
}
