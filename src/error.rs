#[derive(Debug, thiserror::Error)]
pub enum VltError {
    #[error("{0}")]
    InvalidArguments(String),
    #[error("path is outside the vault: {0}")]
    PathOutsideVault(String),
    #[error("document not found: {0}")]
    DocumentNotFound(String),
    #[error("the index does not match the configuration ({0}); run `vlt reindex --force`")]
    IndexRebuildRequired(String),
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("{0}")]
    Model(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Internal(String),
    #[error("the text was not found in the document body")]
    NotFound,
    #[error("the text appears {0} times in the document body; extend it with surrounding text until it is unique")]
    Ambiguous(usize),
    #[error("the file changed while it was being edited; read it again and retry")]
    FileChanged,
    #[error("invalid frontmatter: {0}; ask the user to fix it first")]
    InvalidFrontmatter(String),
    #[error("{0} already exists; choose another path, or process that note")]
    AlreadyExists(String),
}

impl VltError {
    pub fn code(&self) -> &'static str {
        match self {
            VltError::InvalidArguments(_) => "invalid_arguments",
            VltError::PathOutsideVault(_) => "path_outside_vault",
            VltError::DocumentNotFound(_) => "document_not_found",
            VltError::IndexRebuildRequired(_) => "index_rebuild_required",
            VltError::NotFound => "not_found",
            VltError::Ambiguous(_) => "ambiguous",
            VltError::FileChanged => "file_changed",
            VltError::InvalidFrontmatter(_) => "invalid_frontmatter",
            VltError::AlreadyExists(_) => "already_exists",
            _ => "internal",
        }
    }
}

pub type Result<T> = std::result::Result<T, VltError>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codes() {
        assert_eq!(VltError::InvalidArguments("x".into()).code(), "invalid_arguments");
        assert_eq!(VltError::PathOutsideVault("x".into()).code(), "path_outside_vault");
        assert_eq!(VltError::DocumentNotFound("x".into()).code(), "document_not_found");
        assert_eq!(VltError::IndexRebuildRequired("x".into()).code(), "index_rebuild_required");
        assert_eq!(VltError::Config("x".into()).code(), "internal");
        assert_eq!(VltError::Internal("x".into()).code(), "internal");
        assert_eq!(VltError::NotFound.code(), "not_found");
        assert_eq!(VltError::Ambiguous(2).code(), "ambiguous");
        assert_eq!(VltError::FileChanged.code(), "file_changed");
        assert_eq!(VltError::InvalidFrontmatter("x".into()).code(), "invalid_frontmatter");
        assert_eq!(VltError::AlreadyExists("x".into()).code(), "already_exists");
    }
}
