//! Asking for passwords on the terminal, without echoing them.

use anyhow::{Context, Result, bail};

/// Asks for the password of private buckets.
pub fn ask() -> Result<String> {
    rpassword::prompt_password("Password for private buckets: ")
        .context("a password can only be entered on a terminal")
}

/// Asks for a new password twice, so a typo doesn't lock the bucket away.
pub fn new() -> Result<String> {
    let password = rpassword::prompt_password("New password: ")
        .context("a password can only be entered on a terminal")?;
    if password.is_empty() {
        bail!("the password can't be empty");
    }
    let again = rpassword::prompt_password("Again: ")?;
    if password != again {
        bail!("the passwords don't match");
    }
    Ok(password)
}
