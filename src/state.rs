use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type StateHandle = Arc<Mutex<State>>;

#[derive(Default)]
pub struct State {
    pub creds: HashMap<String, String>,
}
