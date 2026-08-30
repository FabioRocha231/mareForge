use thiserror::Error;

#[derive(Debug, Error)]
pub enum SharedError {
    #[error("invalid id")]
    InvalidId,
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use crate::SharedError;

    #[test]
    fn shared_error_implements_std_error() {
        let error = SharedError::InvalidId;
        let _: &dyn std::error::Error = &error;
        assert_eq!(error.to_string(), "invalid id");
    }
}
