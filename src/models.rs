use std::num::NonZeroI64;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// This maps to `LatestData` in MetadataServer & UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LatestResponseV1 {
    pub last_event_id: i64,
    pub last_comment_id: i64,
    pub last_build_id: i64,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum BuildDataResult {
    Starting = 1,
    Failure = 2,
    Warning = 3,
    Success = 4,
    Skipped = 5,
}

/// This maps to `BuildData` in MetadataServer, `BadgeData` in UGS
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Badge {
    pub id: Option<NonZeroI64>,
    pub change_number: u32,
    pub added_at: DateTime<Utc>,
    pub build_type: String,
    pub result: BuildDataResult,
    pub url: Option<String>,
    pub project: String,
    pub archive_path: Option<String>,
}

impl Badge {
    pub fn new(
        change_number: u32,
        build_type: &str,
        result: BuildDataResult,
        url: Option<&str>,
        project: &str,
        archive_path: Option<&str>,
    ) -> Self {
        Self {
            id: None,
            change_number,
            added_at: Utc::now(),
            build_type: build_type.into(),
            result,
            url: url.map(&str::to_owned),
            project: project.to_owned(),
            archive_path: archive_path.map(&str::to_owned),
        }
    }
}
