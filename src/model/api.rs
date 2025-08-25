use serde::Serialize;

#[derive(Serialize)]
pub struct CreateResponse {
    pub id: String,
}
