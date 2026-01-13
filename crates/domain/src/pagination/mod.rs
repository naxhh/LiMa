use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Cursor {
    pub updated_at: String,
    pub id: String,
    pub rank: Option<f64>,
}