//! Private buckets: a bucket's name and tasks, encrypted with a password.
//!
//! The key comes from the password through Argon2id, with a random salt per
//! bucket. The bucket (name, tasks, header) is encrypted as JSON with
//! XChaCha20-Poly1305 under a fresh random nonce each time it's sealed, with
//! the bucket id as associated data, so a sealed bucket can't be passed off
//! as another. Only the header (id and times) stays readable, so syncing can
//! still tell which version of a bucket is newer.
//!
//! There is no way back from a forgotten password: the tasks can't be
//! decrypted without it.

use std::fmt;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::bucket::Bucket;
use crate::utils::error::{Error, Result};
use crate::utils::header::Header;

/// How a key is derived from the password: Argon2id with these costs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kdf {
    /// Random salt, hex.
    pub salt: String,
    /// Memory, in KiB.
    pub m_cost: u32,
    /// Iterations.
    pub t_cost: u32,
    /// Lanes.
    pub p_cost: u32,
}

impl Kdf {
    /// A fresh salt with Argon2's recommended costs (19 MiB, 2 passes).
    pub fn new() -> Result<Kdf> {
        Kdf::with_costs(Params::DEFAULT_M_COST, Params::DEFAULT_T_COST, 1)
    }

    /// A fresh salt with the given costs.
    pub fn with_costs(m_cost: u32, t_cost: u32, p_cost: u32) -> Result<Kdf> {
        let mut salt = [0u8; 16];
        random(&mut salt)?;
        Ok(Kdf {
            salt: hex(&salt),
            m_cost,
            t_cost,
            p_cost,
        })
    }
}

/// A bucket as stored when private: readable header, sealed contents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sealed {
    pub header: Header,
    pub sealed: Ciphertext,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ciphertext {
    pub kdf: Kdf,
    /// Random 24-byte nonce, hex.
    pub nonce: String,
    /// The encrypted bucket JSON with its authentication tag, hex.
    pub data: String,
}

impl Sealed {
    pub fn id(&self) -> Uuid {
        self.header.id
    }
}

/// A key derived from a password for one bucket's salt.
#[derive(Clone)]
pub struct Key {
    bytes: [u8; 32],
    kdf: Kdf,
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key(…)")
    }
}

impl Key {
    /// Derives the key for `password` with `kdf`'s salt and costs.
    pub fn derive(password: &str, kdf: &Kdf) -> Result<Key> {
        if password.is_empty() {
            return Err(Error::EmptyField("password"));
        }
        let salt = unhex(&kdf.salt).ok_or_else(|| crypto("the salt isn't valid hex"))?;
        let params = Params::new(kdf.m_cost, kdf.t_cost, kdf.p_cost, Some(32))
            .map_err(|err| crypto(&err.to_string()))?;
        let mut bytes = [0u8; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.as_bytes(), &salt, &mut bytes)
            .map_err(|err| crypto(&err.to_string()))?;
        Ok(Key {
            bytes,
            kdf: kdf.clone(),
        })
    }
}

/// Encrypts `bucket` with `key`. Sealing the same bucket twice gives
/// different ciphertexts (a fresh nonce each time).
pub fn seal(bucket: &Bucket, key: &Key) -> Result<Sealed> {
    let plaintext = serde_json::to_vec(bucket).map_err(|err| crypto(&err.to_string()))?;
    let mut nonce = [0u8; 24];
    random(&mut nonce)?;
    let id = bucket.header.id;
    let data = cipher(key)?
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &plaintext,
                aad: id.as_bytes(),
            },
        )
        .map_err(|_| crypto("encryption failed"))?;
    Ok(Sealed {
        header: bucket.header,
        sealed: Ciphertext {
            kdf: key.kdf.clone(),
            nonce: hex(&nonce),
            data: hex(&data),
        },
    })
}

/// Decrypts a sealed bucket. Fails with [`Error::WrongPassword`] if `key`
/// isn't the one it was sealed with.
pub fn open(sealed: &Sealed, key: &Key) -> Result<Bucket> {
    let nonce: [u8; 24] = unhex(&sealed.sealed.nonce)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| crypto("the nonce isn't valid"))?;
    let data = unhex(&sealed.sealed.data).ok_or_else(|| crypto("the data isn't valid hex"))?;
    let plaintext = cipher(key)?
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &data,
                aad: sealed.id().as_bytes(),
            },
        )
        .map_err(|_| Error::WrongPassword)?;
    serde_json::from_slice(&plaintext).map_err(|err| crypto(&err.to_string()))
}

fn cipher(key: &Key) -> Result<XChaCha20Poly1305> {
    XChaCha20Poly1305::new_from_slice(&key.bytes).map_err(|_| crypto("bad key length"))
}

fn random(buf: &mut [u8]) -> Result<()> {
    getrandom::fill(buf).map_err(|err| crypto(&err.to_string()))
}

fn crypto(message: &str) -> Error {
    Error::Crypto(message.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::task::Task;

    /// Cheap costs, so tests don't spend time deriving keys.
    pub(crate) fn cheap_kdf() -> Kdf {
        Kdf::with_costs(8, 1, 1).unwrap()
    }

    #[test]
    fn a_sealed_bucket_opens_only_with_its_password() {
        let mut bucket = Bucket::new("diary");
        bucket.add(Task::new("a secret"));
        let kdf = cheap_kdf();
        let key = Key::derive("correct horse", &kdf).unwrap();

        let sealed = seal(&bucket, &key).unwrap();
        assert_eq!(sealed.id(), bucket.header.id);
        let stored = serde_json::to_string(&sealed).unwrap();
        assert!(!stored.contains("diary") && !stored.contains("a secret"));

        assert_eq!(open(&sealed, &key).unwrap(), bucket);
        let wrong = Key::derive("wrong horse", &kdf).unwrap();
        assert!(matches!(open(&sealed, &wrong), Err(Error::WrongPassword)));
    }

    #[test]
    fn sealing_twice_gives_different_ciphertext() {
        let bucket = Bucket::new("diary");
        let key = Key::derive("pw", &cheap_kdf()).unwrap();
        assert_ne!(seal(&bucket, &key).unwrap(), seal(&bucket, &key).unwrap());
    }

    #[test]
    fn a_blob_moved_to_another_bucket_doesnt_open() {
        let bucket = Bucket::new("diary");
        let key = Key::derive("pw", &cheap_kdf()).unwrap();
        let mut sealed = seal(&bucket, &key).unwrap();
        sealed.header.id = Uuid::now_v7();
        assert!(matches!(open(&sealed, &key), Err(Error::WrongPassword)));
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(unhex(&hex(&[0, 1, 254, 255])).unwrap(), [0, 1, 254, 255]);
        assert!(unhex("abc").is_none() && unhex("zz").is_none());
    }
}
