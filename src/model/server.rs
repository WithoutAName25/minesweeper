use serde::Serialize;

use super::{Cell, Pos};

#[derive(Serialize)]
pub struct CellUpdate {
    pub pos: Pos,
    pub value: Cell,
}

#[derive(Serialize)]
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
