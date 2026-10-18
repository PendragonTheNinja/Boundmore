//! Accounts: a name and an Argon2id password hash.
//!
//! A name belongs to one account forever. Names are unique ignoring case (the
//! key is the lowercased name) and keep the spelling they were registered
//! with. Hashing and verifying are slow on purpose, so both run in the
//! connection tasks (`net.rs`), never on the game thread. Passwords are never
//! stored, logged or kept beyond the call that checks them.

use crate::store::{AccountRecord, Store};
use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use bm_protocol::{MAX_PASSWORD_LEN, MIN_PASSWORD_LEN, clean_name};
use std::sync::OnceLock;

/// A logged-in account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// The lowercased name: what saves are keyed by.
    pub key: String,
    /// The name as registered: what players see.
    pub name: String,
}

#[derive(Debug)]
pub enum LoginError {
    BadName,
    BadPassword,
    NameTaken,
    /// Unknown name or wrong password: deliberately the same answer.
    WrongCredentials,
    /// This address failed to log in too often lately.
    TooManyAttempts,
    Server(anyhow::Error),
}

impl LoginError {
    /// The reason shown to the player.
    pub fn reason(&self) -> String {
        match self {
            LoginError::BadName => "that name cannot be used (2 to 24 letters, digits, spaces, - or _)".into(),
            LoginError::BadPassword => {
                format!("a password must be {MIN_PASSWORD_LEN} to {MAX_PASSWORD_LEN} characters")
            }
            LoginError::NameTaken => "that name is taken".into(),
            LoginError::WrongCredentials => "wrong name or password".into(),
            LoginError::TooManyAttempts => "too many failed logins from your address; wait a minute".into(),
            LoginError::Server(_) => "the server could not check your account; try again".into(),
        }
    }
}

impl From<anyhow::Error> for LoginError {
    fn from(e: anyhow::Error) -> Self {
        LoginError::Server(e)
    }
}

/// The key a name is stored under.
pub fn key(name: &str) -> String {
    name.to_lowercase()
}

fn password_ok(password: &str) -> bool {
    (MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&password.chars().count())
}

fn hash(password: &str) -> Result<String, LoginError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| LoginError::Server(anyhow::anyhow!("hashing failed: {e}")))
}

/// A hash no password is known for, checked when the name does not exist so
/// a wrong name takes as long to refuse as a wrong password.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash("not a password anyone has").unwrap_or_default())
}

fn verify(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

/// Creates an account. Slow (hashes the password).
pub fn register(store: &Store, name: &str, password: &str) -> Result<Account, LoginError> {
    let name = clean_name(name).ok_or(LoginError::BadName)?;
    if !password_ok(password) {
        return Err(LoginError::BadPassword);
    }
    let key = key(&name);
    // A cheap look first, so a taken name costs no hashing; `create_account`
    // checks again atomically.
    if store.account(&key)?.is_some() {
        return Err(LoginError::NameTaken);
    }
    let record = AccountRecord {
        name: name.clone(),
        hash: hash(password)?,
    };
    if !store.create_account(&key, &record)? {
        return Err(LoginError::NameTaken);
    }
    tracing::info!("account `{name}` registered");
    Ok(Account { key, name })
}

/// Checks a name and password. Slow (verifies a hash) whether or not the
/// name exists.
pub fn login(store: &Store, name: &str, password: &str) -> Result<Account, LoginError> {
    let (Some(name), true) = (clean_name(name), password.chars().count() <= MAX_PASSWORD_LEN) else {
        return Err(LoginError::WrongCredentials);
    };
    let key = key(&name);
    let record = store.account(&key)?;
    let phc = record.as_ref().map_or(dummy_hash(), |r| r.hash.as_str());
    let ok = verify(password, phc);
    match record {
        Some(r) if ok => Ok(Account { key, name: r.name }),
        _ => Err(LoginError::WrongCredentials),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("p.redb")).unwrap();
        (dir, s)
    }

    #[test]
    fn register_then_login_ignoring_case() {
        let (_d, s) = store();
        let a = register(&s, "  Pen  Dragon ", "correct horse").unwrap();
        assert_eq!(a.name, "Pen Dragon");
        assert_eq!(a.key, "pen dragon");
        let rec = s.account("pen dragon").unwrap().unwrap();
        assert!(rec.hash.starts_with("$argon2id$"), "argon2id PHC string");
        assert!(!rec.hash.contains("correct horse"));
        let b = login(&s, "pen dragon", "correct horse").unwrap();
        assert_eq!(b, a, "keeps the registered spelling");
        assert!(matches!(
            login(&s, "Pen Dragon", "wrong horse"),
            Err(LoginError::WrongCredentials)
        ));
        assert!(matches!(
            login(&s, "Nobody", "correct horse"),
            Err(LoginError::WrongCredentials)
        ));
    }

    #[test]
    fn names_are_taken_forever_and_rules_hold() {
        let (_d, s) = store();
        register(&s, "Alice", "password1").unwrap();
        assert!(matches!(register(&s, "ALICE", "password2"), Err(LoginError::NameTaken)));
        assert!(matches!(register(&s, "x", "password1"), Err(LoginError::BadName)));
        assert!(matches!(register(&s, "Bob", "short"), Err(LoginError::BadPassword)));
        assert!(matches!(
            register(&s, "Bob", &"p".repeat(MAX_PASSWORD_LEN + 1)),
            Err(LoginError::BadPassword)
        ));
        assert!(matches!(
            login(&s, "Alice", &"p".repeat(MAX_PASSWORD_LEN + 1)),
            Err(LoginError::WrongCredentials)
        ));
    }
}
