use std::sync::Arc;
use std::time::Duration;

use rmcp::service::serve_server;

use crate::app::App;
use crate::{Result, VltError};

pub mod tools;

/// Serves the MCP tools on stdio until the client disconnects.
/// Configuration errors are returned before the transport starts; models load on first use.
pub fn serve() -> Result<()> {
    let app = Arc::new(App::load()?);
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, async move {
        tokio::spawn(unload_idle_models(app.clone()));
        let server = tools::VltServer::new(app);
        let running = serve_server(server, (tokio::io::stdin(), tokio::io::stdout()))
            .await
            .map_err(|e| VltError::Internal(format!("mcp: {e}")))?;
        let _ = running.waiting().await;
        Ok(())
    })
}

/// Drops the loaded models whenever they have been unused for `idle_unload_secs`.
async fn unload_idle_models(app: Arc<App>) {
    let idle_secs = app.cfg.models.idle_unload_secs;
    let mut tick = tokio::time::interval(unload_interval(idle_secs));
    loop {
        tick.tick().await;
        app.models.unload_if_idle(Duration::from_secs(idle_secs));
    }
}

/// How often the background task checks for idle models.
fn unload_interval(idle_unload_secs: u64) -> Duration {
    Duration::from_secs(idle_unload_secs.clamp(1, 30))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_check_runs_every_min_of_idle_and_30_seconds() {
        assert_eq!(unload_interval(300), Duration::from_secs(30));
        assert_eq!(unload_interval(31), Duration::from_secs(30));
        assert_eq!(unload_interval(30), Duration::from_secs(30));
        assert_eq!(unload_interval(10), Duration::from_secs(10));
    }

    #[test]
    fn idle_models_are_unloaded_in_the_background() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::Config::parse(&crate::config::Config::default_toml(dir.path())).unwrap();
        cfg.models.idle_unload_secs = 1;
        let app = Arc::new(App::new(dir.path().to_path_buf(), cfg, true).unwrap());
        app.models.embedder().unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            tokio::spawn(unload_idle_models(app.clone()));
            tokio::time::sleep(Duration::from_millis(500)).await;
            assert!(app.models.is_loaded(), "still within the idle window");
            tokio::time::sleep(Duration::from_millis(1000)).await;
            assert!(!app.models.is_loaded());
        });
    }

    #[test]
    fn idle_check_interval_is_never_zero() {
        assert_eq!(unload_interval(0), Duration::from_secs(1));
    }
}
