use anyhow::Context as _;
use argon2::{Argon2, PasswordHash, PasswordVerifier as _};
use std::path::Path;

pub fn verify_password(password: &str, password_file: &Path) -> anyhow::Result<bool> {
    let expected = std::fs::read_to_string(password_file)?;
    let hash = PasswordHash::new(expected.trim()).context("Invalid password hash")?;
    // TODO: Reuse argon2 instance?
    let argon2 = Argon2::default();
    match argon2.verify_password(password.as_bytes(), &hash) {
        Ok(_) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(err) => Err(err.into()),
    }
}
