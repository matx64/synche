use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct AppPorts {
    pub http: u16,
    pub presence: u16,
    pub transport: u16,
}
