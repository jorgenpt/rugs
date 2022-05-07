use std::num::NonZeroI64;

use chrono::{DateTime, Utc};
use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// This maps to `LatestData` in MetadataServer & UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LatestResponseV1 {
    pub last_event_id: i64,
    pub last_comment_id: i64,
    pub last_build_id: i64,
}

#[derive(
    Clone, Copy, PartialEq, Debug, Serialize_repr, Deserialize_repr, FromPrimitive, ToPrimitive,
)]
#[repr(u8)]
pub enum BuildDataResult {
    Starting = 0,
    Failure = 1,
    Warning = 2,
    Success = 3,
    Skipped = 4,
}

/// This maps to `BuildData` in MetadataServer, `BadgeData` in UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Badge {
    pub id: Option<NonZeroI64>,
    pub change_number: i64,
    pub added_at: DateTime<Utc>,
    pub build_type: String,
    pub result: BuildDataResult,
    pub url: String,
    pub project: String,
    pub archive_path: Option<String>,
}

/// This maps to `BuildData` in MetadataServer, `BadgeData` in UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateBadge {
    pub change_number: i64,
    pub build_type: String,
    pub result: BuildDataResult,
    pub url: String,
    pub project: String,
    pub archive_path: Option<String>,
}
