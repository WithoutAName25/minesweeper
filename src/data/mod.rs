#[derive(Debug, PartialEq, Eq)]
pub enum RevealedState {
    Hidden,
    Marked,
    Flagged,
    Revealed,
}

#[derive(Debug)]
pub struct Cell {
    pub bomb: bool,
    pub adjacent: u8,
    pub revealed: RevealedState,
}

#[derive(Debug)]
pub struct Field {
    pub width: usize,
    pub height: usize,
    pub bombs: usize,
    pub revealed: usize,
    pub finished: bool,
    pub cells: Vec<Cell>,
}
