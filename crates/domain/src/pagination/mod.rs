use serde::{Serialize, Deserialize};
use base64::{Engine as _, engine::general_purpose};

#[derive(Debug, Serialize, Deserialize)]
pub struct Cursor {
    pub updated_at: String,
    pub id: String,
    pub rank: Option<f64>,
}

pub fn decode_cursor(cursor: &str) -> Result<Cursor, String> {
    let bytes = general_purpose::STANDARD.decode(cursor).map_err(|e| e.to_string())?;
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|e: serde_json::Error| e.to_string())?;
    
    Ok(cursor)
}

pub fn encode_cursor(cursor: &Cursor) -> String {
    general_purpose::STANDARD.encode(serde_json::to_vec(&cursor).unwrap())
}
