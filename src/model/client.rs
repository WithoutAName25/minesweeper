use serde::Deserialize;

use super::{GameParams, Pos};

#[derive(Deserialize)]
#[serde(tag = "action")]
pub enum ClientMessage {
    #[serde(rename = "reveal")]
    Reveal { pos: Pos },
    #[serde(rename = "flag")]
    Flag { pos: Pos },
    #[serde(rename = "restart")]
    Restart { params: GameParams },
}
