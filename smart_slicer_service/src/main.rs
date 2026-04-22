mod handlers;
mod routes;
mod types;
mod utils;

use axum::middleware::from_fn;
use axum::{extract::DefaultBodyLimit, http::HeaderName, response::IntoResponse};
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utils::{LOG_DIR, SERVER_PORT, logging};

async fn not_found() -> impl IntoResponse {
    types::error_types::AppError::NotFound
}

#[tokio::main]
async fn main() {
    let file_appender = tracing_appender::rolling::daily(LOG_DIR.as_str(), "app.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(file_writer),
        )
        .init();

    let app = routes::create_router()
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(from_fn(logging::logger))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::new(
            HeaderName::from_static("x-request-id"),
            MakeRequestUuid::default(),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let addr = format!("0.0.0.0:{}", SERVER_PORT.as_str());
    tracing::info!("Server running on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
