pub mod archive;
pub mod github;
pub mod instapaper;
pub mod readability;
pub mod youtube;

pub struct ExtractedArticle {
    pub title: String,
    pub content: String,
}
