use axum::{
    Router,
    routing::{get, post},
};

use crate::handlers;

pub fn unguarded_routes() -> Router {
    Router::new()
        .route("/api/health", get(handlers::health_handler::health))
        .route("/api/slice", post(handlers::slicer_handlers::slice))
}

pub fn create_router() -> Router {
    Router::new().merge(unguarded_routes())
}
