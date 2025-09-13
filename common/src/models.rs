use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(tag = "state")]
pub enum Cell {
    #[serde(rename = "hidden")]
    Hidden,
    #[serde(rename = "marked")]
    Marked,
    #[serde(rename = "flagged")]
    Flagged,
    #[serde(rename = "revealed")]
    Revealed { adjacent: u8 },
    #[serde(rename = "bomb")]
    Bomb,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Pos {
    pub x: usize,
    pub y: usize,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct GameParams {
    pub width: usize,
    pub height: usize,
    pub bombs: usize,
}

impl Default for GameParams {
    fn default() -> Self {
        Self {
            width: 9,
            height: 9,
            bombs: 10,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateResponse {
    pub id: String,
}
