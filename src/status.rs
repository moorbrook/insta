/// Persistence status stored in `articles.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArticleStatus {
    Pending,
    Success,
    Archived,
    Failed,
}

impl ArticleStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Archived => "archived",
            Self::Failed => "failed",
        }
    }
}

/// Successful extraction that either came from the live site or an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SuccessSource {
    Live,
    Archive,
}

impl SuccessSource {
    pub(crate) const fn status(self) -> ArticleStatus {
        match self {
            Self::Live => ArticleStatus::Success,
            Self::Archive => ArticleStatus::Archived,
        }
    }
}
