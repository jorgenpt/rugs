use axum::{
    routing::{get},
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
};

use std::{net::SocketAddr, num::NonZeroI64};

mod models;

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        .route("/api/latest", get(latest))
        .route("/api/build", get(badges))
        .route("/api/issues", get(issues));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn latest() -> impl IntoResponse {
    let response = models::LatestResponseV1 {
        last_build_id: 0,
        last_comment_id: 0,
        last_event_id: 0,
    };
    (StatusCode::OK, Json(response))
}

async fn badges() -> impl IntoResponse {
    let response = [
        models::Badge {
            id: NonZeroI64::new(1),
            change_number: 11719,
            added_at: chrono::Utc::now(),
            build_type: "Editor".into(),
            result: models::BuildDataResult::Success,
            url: Some("http://google.com".into()),
            project: "//Test/main/Test/Test.uproject".into(),
            archive_path: Some("//Test".into()),
        }
    ];
    (StatusCode::OK, Json(response))
}

async fn issues() -> impl IntoResponse {
    let response : [&str; 0] = [];
    // Unimplemented for now
    (StatusCode::OK, Json(response))
}