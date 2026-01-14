use axum::{Json, response::IntoResponse, http::StatusCode, response::Response};
use serde::Serialize;
use utoipa::ToSchema;



#[derive(Debug, Serialize, ToSchema)]
pub struct ApiErrorBody {
    pub error: ApiError,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub request_id: Option<String>,
}

#[derive(Debug)]
pub struct ApiErrorResponse {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: &'static str,
    pub details: Option<serde_json::Value>,
}

impl ApiErrorResponse {
    pub fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
            details: None,
        }
    }

    pub fn with_cause(self, cause: &str) -> Self {
        let details = serde_json::json!({ "cause": cause });
        self.with_details(details)
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        let body = ApiErrorBody {
            error: ApiError {
                code: self.code.to_string(),
                message: self.message.to_string(),
                details: self.details,
                request_id: None,
            },
        };

        (self.status, Json(body)).into_response()
    }
}