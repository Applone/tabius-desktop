use crate::Result;
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Sha256, Sha512};
use zeroize::Zeroizing;

const LIMIT: usize = 8 * 1024 * 1024;
const DAMAGED: &str = "The vault password is incorrect or the vault is damaged.";
const KEY_INFO: &[u8] = b"tabius/vault/v2/encryption-key";
const HEADER: &[u8] =
    b"tabius-vault-v2\0argon2id-v19\0m=65536,t=3,p=4\0hkdf-sha256\0xchacha20-poly1305\0";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Encryption {
    Modern,
    Tabby,
}

fn metadata() -> Value {
    json!({
        "version":2,
        "cipher":"xchacha20-poly1305",
        "kdf":{"name":"argon2id","version":19,"memoryKiB":65536,"iterations":3,"parallelism":4},
        "keyDerivation":"hkdf-sha256"
    })
}

fn derive_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let mut master = Zeroizing::new([0u8; 32]);
    let params = Params::new(65536, 3, 4, Some(32)).map_err(|e| e.to_string())?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, master.as_mut())
        .map_err(|_| "Cannot derive the vault key.")?;
    let mut key = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(None, master.as_ref())
        .expand(KEY_INFO, key.as_mut())
        .map_err(|_| "Cannot derive the vault encryption key.")?;
    Ok(key)
}

fn aad(salt: &[u8], nonce: &[u8]) -> Vec<u8> {
    [HEADER, salt, nonce].concat()
}

pub fn decrypt(envelope: &Value, password: &str) -> Result<Value> {
    if password.is_empty() {
        return Err("Enter the vault password.".into());
    }
    let plain = match envelope["version"].as_u64() {
        Some(1) => return decrypt_legacy(envelope, password),
        Some(2) => {
            let mut header = envelope.clone();
            let fields = header.as_object_mut().ok_or("Invalid vault envelope.")?;
            for field in ["keySalt", "nonce", "contents"] {
                fields.remove(field);
            }
            // A v2 suite has fixed costs. Never allocate memory based on remote KDF parameters.
            if header != metadata() {
                return Err("Unsupported modern vault algorithms or parameters.".into());
            }
            let salt = unhex(&envelope["keySalt"], 16)?;
            let nonce = unhex(&envelope["nonce"], 24)?;
            let bytes = ciphertext(envelope)?;
            if bytes.len() < 16 {
                return Err("Invalid vault ciphertext.".into());
            }
            let key = derive_key(password, &salt)?;
            XChaCha20Poly1305::new_from_slice(key.as_ref())
                .map_err(|_| "Invalid vault key.")?
                .decrypt(
                    &XNonce::try_from(nonce.as_slice()).map_err(|_| "Invalid vault nonce.")?,
                    Payload {
                        msg: &bytes,
                        aad: &aad(&salt, &nonce),
                    },
                )
                .map_err(|_| DAMAGED)?
        }
        _ => return Err("Unsupported vault version.".into()),
    };
    parse_plaintext(&Zeroizing::new(plain))
}

fn parse_plaintext(plain: &[u8]) -> Result<Value> {
    let value: Value = serde_json::from_slice(plain).map_err(|_| DAMAGED)?;
    if !value.is_object() {
        return Err("Invalid vault contents: expected an object.".into());
    }
    Ok(value)
}

fn ciphertext(envelope: &Value) -> Result<Vec<u8>> {
    let contents = envelope["contents"]
        .as_str()
        .ok_or("Invalid vault ciphertext.")?;
    if contents.len() > (LIMIT + 16).div_ceil(3) * 4 {
        return Err("Vault exceeds 8 MB.".into());
    }
    let bytes = STANDARD
        .decode(contents)
        .or_else(|_| STANDARD_NO_PAD.decode(contents))
        .map_err(|_| "Invalid vault base64.")?;
    if bytes.len() > LIMIT + 16 {
        return Err("Vault exceeds 8 MB.".into());
    }
    Ok(bytes)
}

pub fn encrypt(vault: &Value, password: &str, encryption: Encryption) -> Result<Value> {
    if password.is_empty() {
        return Err("Enter the vault password.".into());
    }
    if !vault.is_object() {
        return Err("Invalid vault contents: expected an object.".into());
    }
    if encryption == Encryption::Tabby {
        return encrypt_legacy(vault, password);
    }
    let plain = Zeroizing::new(serde_json::to_vec(vault).map_err(|e| e.to_string())?);
    if plain.len() > LIMIT {
        return Err("Vault exceeds 8 MB.".into());
    }
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    getrandom::fill(&mut salt).map_err(|_| "Cannot generate vault randomness.")?;
    getrandom::fill(&mut nonce).map_err(|_| "Cannot generate vault randomness.")?;
    let key = derive_key(password, &salt)?;
    let bytes = XChaCha20Poly1305::new_from_slice(key.as_ref())
        .map_err(|_| "Invalid vault key.")?
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &plain,
                aad: &aad(&salt, &nonce),
            },
        )
        .map_err(|_| "Cannot encrypt the vault.")?;
    let mut envelope = metadata();
    envelope["keySalt"] = json!(hex(&salt));
    envelope["nonce"] = json!(hex(&nonce));
    envelope["contents"] = json!(STANDARD.encode(bytes));
    Ok(envelope)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(value: &Value, size: usize) -> Result<Vec<u8>> {
    let s = value.as_str().ok_or("Invalid vault format.")?;
    if s.len() != size * 2 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid vault format.".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn decrypt_legacy(envelope: &Value, password: &str) -> Result<Value> {
    if password.is_empty() {
        return Err("Enter the Tabby vault password.".into());
    }
    if envelope["version"] != 1 {
        return Err("Unsupported Tabby vault version.".into());
    }
    let salt = unhex(&envelope["keySalt"], 8)?;
    let iv = unhex(&envelope["iv"], 16)?;
    let mut bytes = Zeroizing::new(ciphertext(envelope)?);
    if bytes.is_empty() || bytes.len() > LIMIT || !bytes.len().is_multiple_of(16) {
        return Err("Invalid Tabby vault content.".into());
    }
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha512>(password.as_bytes(), &salt, 100_000, key.as_mut());
    let plain = cbc::Decryptor::<aes::Aes256>::new_from_slices(key.as_ref(), &iv)
        .map_err(|e| e.to_string())?
        .decrypt_padded_mut::<Pkcs7>(&mut bytes)
        .map_err(|_| "The Tabby vault password is incorrect or the vault is damaged.")?;
    parse_plaintext(plain)
}
fn encrypt_legacy(vault: &Value, password: &str) -> Result<Value> {
    let mut bytes = Zeroizing::new(serde_json::to_vec(vault).map_err(|e| e.to_string())?);
    let length = bytes.len();
    if length >= LIMIT {
        return Err("Tabby vault exceeds 8 MB.".into());
    }
    let mut salt = [0u8; 8];
    let mut iv = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| e.to_string())?;
    getrandom::fill(&mut iv).map_err(|e| e.to_string())?;
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha512>(password.as_bytes(), &salt, 100_000, key.as_mut());
    bytes.resize(length + 16, 0);
    let encrypted = cbc::Encryptor::<aes::Aes256>::new_from_slices(key.as_ref(), &iv)
        .map_err(|e| e.to_string())?
        .encrypt_padded_mut::<Pkcs7>(&mut bytes, length)
        .map_err(|e| e.to_string())?;
    Ok(
        json!({"version":1,"keySalt":hex(&salt),"iv":hex(&iv),"contents":STANDARD.encode(encrypted)}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("fixtures/tabius-vault-v2.json")).unwrap()
    }

    #[test]
    fn reads_independent_argon2_hkdf_libsodium_fixture() {
        let f = fixture();
        let password = f["password"].as_str().unwrap();
        let salt = unhex(&f["envelope"]["keySalt"], 16).unwrap();
        let key = derive_key(password, &salt).unwrap();
        assert_eq!(hex(key.as_ref()), f["hkdfOutput"]);
        assert_eq!(decrypt(&f["envelope"], password).unwrap(), f["plaintext"]);
    }

    #[test]
    fn modern_roundtrip_has_fresh_salts_nonces_and_ciphertext() {
        let plain = json!({"config":{"profiles":[]},"secrets":[{"value":"café"}]});
        let a = encrypt(&plain, "password", Encryption::Modern).unwrap();
        let b = encrypt(&plain, "password", Encryption::Modern).unwrap();
        assert_eq!(a["version"], 2);
        for field in ["keySalt", "nonce", "contents"] {
            assert_ne!(a[field], b[field]);
        }
        assert_eq!(decrypt(&a, "password").unwrap(), plain);
        assert_eq!(decrypt(&b, "password").unwrap(), plain);
        assert!(decrypt(&a, "wrong").is_err());
    }

    #[test]
    fn rejects_tampered_ciphertext_tag_salt_nonce_and_password() {
        let f = fixture();
        let password = f["password"].as_str().unwrap();
        let envelope = &f["envelope"];
        for field in ["keySalt", "nonce", "contents"] {
            let mut bad = envelope.clone();
            let mut bytes = if field == "contents" {
                ciphertext(envelope).unwrap()
            } else {
                unhex(&envelope[field], if field == "nonce" { 24 } else { 16 }).unwrap()
            };
            bytes[0] ^= 1;
            bad[field] = json!(if field == "contents" {
                STANDARD.encode(bytes)
            } else {
                hex(&bytes)
            });
            assert_eq!(decrypt(&bad, password).unwrap_err(), DAMAGED);
        }
        let mut bytes = ciphertext(envelope).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        let mut bad = envelope.clone();
        bad["contents"] = json!(STANDARD.encode(&bytes));
        assert_eq!(decrypt(&bad, password).unwrap_err(), DAMAGED);
        bytes.truncate(bytes.len() - 1);
        bad["contents"] = json!(STANDARD.encode(bytes));
        assert!(decrypt(&bad, password).is_err());
        assert_eq!(decrypt(envelope, "wrong").unwrap_err(), DAMAGED);
    }

    #[test]
    fn rejects_unknown_or_expensive_parameters_and_malformed_envelopes() {
        let f = fixture();
        let password = f["password"].as_str().unwrap();
        let original = &f["envelope"];
        for (path, value) in [
            ("/version", json!(3)),
            ("/version", json!(1)),
            ("/cipher", json!("aes-256-cbc")),
            ("/keyDerivation", json!("none")),
            ("/kdf/name", json!("argon2i")),
            ("/kdf/version", json!(16)),
            ("/kdf/memoryKiB", json!(u32::MAX)),
            ("/kdf/memoryKiB", json!(8192)),
            ("/kdf/iterations", json!(u32::MAX)),
            ("/kdf/parallelism", json!(u32::MAX)),
            ("/keySalt", json!("00")),
            ("/nonce", json!("z".repeat(48))),
            ("/contents", json!("invalid!")),
            ("/contents", json!(STANDARD.encode([0u8; 15]))),
            (
                "/contents",
                json!("A".repeat((LIMIT + 16).div_ceil(3) * 4 + 1)),
            ),
        ] {
            let mut bad = original.clone();
            *bad.pointer_mut(path).unwrap() = value;
            assert!(decrypt(&bad, password).is_err(), "{path}");
        }
        for field in [
            "version",
            "cipher",
            "kdf",
            "keyDerivation",
            "keySalt",
            "nonce",
            "contents",
        ] {
            let mut bad = original.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(decrypt(&bad, password).is_err(), "{field}");
        }
        let mut bad = original.clone();
        bad["unexpected"] = json!(true);
        assert!(decrypt(&bad, password).is_err());
    }

    #[test]
    fn validates_plaintext_passwords_and_size_before_encrypting() {
        for mode in [Encryption::Modern, Encryption::Tabby] {
            assert!(encrypt(&json!({}), "", mode).is_err());
            assert!(encrypt(&json!([]), "pw", mode).is_err());
            assert!(encrypt(&json!({"large":"a".repeat(LIMIT)}), "pw", mode).is_err());
        }
        assert!(decrypt(&fixture()["envelope"], "").is_err());
        assert!(parse_plaintext(b"[]").is_err());
        assert!(parse_plaintext(b"invalid").is_err());
    }
}
