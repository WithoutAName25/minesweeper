use serde::{Deserialize, Serialize};

use crate::models::{Cell, GameParams, Pos};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action")]
pub enum ClientMessage {
    #[serde(rename = "reveal")]
    Reveal { pos: Pos },
    #[serde(rename = "flag")]
    Flag { pos: Pos },
    #[serde(rename = "restart")]
    Restart { params: GameParams },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CellUpdate {
    pub pos: Pos,
    pub value: Cell,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "init")]
    Init {
        width: usize,
        height: usize,
        bombs: usize,
        field: Vec<Vec<Cell>>,
    },
    #[serde(rename = "update")]
    Update {
        updates: Vec<CellUpdate>,
        won: bool,
        lost: bool,
    },
}
