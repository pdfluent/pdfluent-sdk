//! XFA API Server — REST API for XFA form processing.
//!
//! Provides HTTP endpoints for extracting, filling, flattening, and
//! validating XFA PDF forms via a JSON-first API.

mod error;
mod routes;
mod state;

use axum::routing::{get, post};
use axum::Router;
use state::AppState;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Build the API router with all endpoints.
pub fn build_router() -> Router {
    let state = AppState::new();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Health check
        .route("/health", get(routes::health))
        // Form processing endpoints
        .route("/api/v1/forms/extract", post(routes::extract_fields))
        .route("/api/v1/forms/fill", post(routes::fill_form))
        .route("/api/v1/forms/validate", post(routes::validate_form))
        .route("/api/v1/forms/{id}/schema", get(routes::get_schema))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "xfa_api_server=info,tower_http=info".into()),
        )
        .init();

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("XFA API Server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, build_router()).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_check() {
        let app = build_router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn extract_without_file_returns_400() {
        let app = build_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/forms/extract")
                    .header("content-type", "multipart/form-data; boundary=boundary")
                    .body(Body::from("--boundary--\r\n"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn validate_without_file_returns_400() {
        let app = build_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/forms/validate")
                    .header("content-type", "multipart/form-data; boundary=boundary")
                    .body(Body::from("--boundary--\r\n"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn schema_unknown_id_returns_404() {
        let app = build_router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/forms/nonexistent/schema")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let app = build_router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Axum returns 404 for unknown routes
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
