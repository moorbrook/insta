use serde::Deserialize;
use std::path::Path;

use crate::error::CsvError;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ArticleRow {
    #[serde(rename = "URL")]
    pub url: String,
    #[serde(rename = "Title")]
    pub title: String,
    #[serde(rename = "Selection")]
    pub(crate) _selection: String,
    #[serde(rename = "Folder")]
    pub folder: String,
    #[serde(rename = "Timestamp")]
    pub timestamp: String,
    #[serde(rename = "Tags")]
    pub tags: String,
}

pub(crate) fn read_csv(path: &Path) -> Result<Vec<ArticleRow>, CsvError> {
    let mut reader = csv::Reader::from_path(path).map_err(|source| {
        if path.exists() {
            CsvError::Read { source }
        } else {
            CsvError::NotFound {
                path: path.to_path_buf(),
            }
        }
    })?;
    let mut articles = Vec::new();
    for result in reader.deserialize() {
        let row: ArticleRow = result.map_err(|source| CsvError::Invalid { source })?;
        articles.push(row);
    }
    Ok(articles)
}
