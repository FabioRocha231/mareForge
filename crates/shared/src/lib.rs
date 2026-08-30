//! shared: tipos comuns não-replicados

pub mod error;
pub mod ids;

pub use error::SharedError;
pub use ids::*;

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert_eq!(1 + 1, 2);
    }
}
