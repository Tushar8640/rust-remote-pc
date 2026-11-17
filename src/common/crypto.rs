use sha2::{Digest, Sha256};

/// Computes a SHA-256 hash string for authentication
pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

/// Verifies whether provided token hash matches the expected password hash
pub fn verify_token(password: &str, provided_hash: &str) -> bool {
    let expected_hash = hash_password(password);
    expected_hash.eq_ignore_ascii_case(provided_hash)
}
