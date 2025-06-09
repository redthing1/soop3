// server module public api

pub mod app;
pub mod handlers;
pub mod middleware;

pub use app::start_server;

// expose create_test_app for integration tests - conditional compilation
// ensures it's only available during testing
#[cfg(any(test, feature = "testing"))]
pub use app::create_test_app;
