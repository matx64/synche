use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ConfiguredDirectory {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directory {
    pub name: String,
}
