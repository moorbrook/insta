use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct ArticleRow {
    #[serde(rename = "URL")]
    pub url: String,
    #[serde(rename = "Title")]
    pub title: String,
    #[serde(rename = "Selection")]
    pub selection: String,
    #[serde(rename = "Folder")]
    pub folder: String,
    #[serde(rename = "Timestamp")]
    pub timestamp: String,
    #[serde(rename = "Tags")]
    pub tags: String,
}

pub fn read_csv(path: &Path) -> anyhow::Result<Vec<ArticleRow>> {
    let mut reader = csv::Reader::from_path(path)?;
    let mut articles = Vec::new();
    for result in reader.deserialize() {
        let row: ArticleRow = result?;
        articles.push(row);
    }
    Ok(articles)
}
