use std::time::Duration;

use async_sqlx_session::SqliteSessionStore;
use tokio::time::sleep;

/// A timer that automagically cleans up the session store
pub async fn session_store_cleanup(timer: Duration, session_store: SqliteSessionStore) {
    loop {
        match session_store.cleanup().await {
            Ok(_) => log::debug!("Cleaned up auth session store"),
            Err(err) => log::error!("Failed to clean up auth session store: {err:?}"),
        };
        // TODO: make the session store cleanup timer configurable in config?
        sleep(timer).await;
    }
}
