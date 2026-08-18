use anyhow::Context as _;
use argon2::{Argon2, PasswordHash, PasswordVerifier as _};

pub fn verify_password(password: &str, expected: &str) -> anyhow::Result<bool> {
    let hash = PasswordHash::new(expected.trim()).context("Invalid password hash")?;
    // TODO: Reuse argon2 instance?
    let argon2 = Argon2::default();
    match argon2.verify_password(password.as_bytes(), &hash) {
        Ok(_) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(err) => Err(err.into()),
    }
}
