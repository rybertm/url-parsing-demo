use serde::Deserialize;

#[derive(Deserialize)]
pub struct UrlBody {
    pub url: String,
}
