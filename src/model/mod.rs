use serde::{Deserialize, Serialize};

pub mod api;
pub mod client;
pub mod server;

#[derive(Clone, Serialize)]
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

#[derive(Deserialize, Serialize, Clone, Copy)]
pub struct Pos {
    pub x: usize,
    pub y: usize,
}

#[derive(Deserialize)]
pub struct GameParams {
    pub width: usize,
    pub height: usize,
    pub bombs: usize,
}
