pub type AppState = std::sync::Arc<std::sync::Mutex<AppStateInner>>;

pub struct AppStateInner {
    pub db: Option<crate::services::database::Database>,
}

impl AppStateInner {
    pub fn new() -> Self {
        Self { db: None }
    }

    pub fn set_db(&mut self, db: crate::services::database::Database) {
        self.db = Some(db);
    }

    pub fn db(&self) -> &crate::services::database::Database {
        self.db.as_ref().expect("Database not initialized")
    }
}

impl Default for AppStateInner {
    fn default() -> Self {
        Self::new()
    }
}
