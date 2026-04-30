use axum::{
    http::StatusCode,
    routing::post,
    Json, Router,
};

use super::{is_valid_domain, AppState};
use crate::services::backup_drill;

type ApiErr = (StatusCode, Json<serde_json::Value>);

fn err(status: StatusCode, msg: &str) -> ApiErr {
    (status, Json(serde_json::json!({ "error": msg })))
}

#[derive(serde::Deserialize)]
pub struct DrillSiteRequest {
    pub domain: String,
    pub filename: String,
}

/// POST /backups/drill/site — End-to-end site drill.
async fn drill_site(
    Json(req): Json<DrillSiteRequest>,
) -> Result<Json<backup_drill::DrillResult>, ApiErr> {
    if !is_valid_domain(&req.domain) {
        return Err(err(StatusCode::BAD_REQUEST, "Invalid domain"));
    }
    if req.filename.is_empty() || req.filename.contains("..") || req.filename.contains('/') {
        return Err(err(StatusCode::BAD_REQUEST, "Invalid filename"));
    }
    let result = backup_drill::drill_site_backup(&req.domain, &req.filename)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e))?;
    Ok(Json(result))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/backups/drill/site", post(drill_site))
}
