//! Password protection: write an AES-128 encrypted copy of the document.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::{EncryptionState, EncryptionVersion, Object, Permissions, StringFormat};

use crate::document::Document;
use crate::error::{Error, Result};

/// What a reader opening the file with the user password may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecurityPermissions {
    pub print: bool,
    pub modify: bool,
    pub copy: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    pub assemble: bool,
}

impl Default for SecurityPermissions {
    fn default() -> Self {
        SecurityPermissions {
            print: true,
            modify: true,
            copy: true,
            annotate: true,
            fill_forms: true,
            assemble: true,
        }
    }
}

impl SecurityPermissions {
    fn to_lopdf(self) -> Permissions {
        let mut p = Permissions::empty();
        if self.print {
            p |= Permissions::PRINTABLE;
        }
        if self.modify {
            p |= Permissions::MODIFIABLE;
        }
        if self.copy {
            p |= Permissions::COPYABLE | Permissions::COPYABLE_FOR_ACCESSIBILITY;
        }
        if self.annotate {
            p |= Permissions::ANNOTABLE;
        }
        if self.fill_forms {
            p |= Permissions::FILLABLE;
        }
        if self.assemble {
            p |= Permissions::ASSEMBLABLE;
        }
        p
    }
}

/// Sixteen pseudo-random bytes for the trailer `/ID` (time and address entropy).
fn file_id() -> Vec<u8> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let marker = Box::new(0u8);
    let addr = &*marker as *const u8 as usize as u128;
    let mut x = nanos ^ addr.rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835;
    let mut out = Vec::with_capacity(16);
    for _ in 0..16 {
        // xorshift-style mixing, good enough for a document identifier
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push((x & 0xff) as u8);
    }
    out
}

impl Document {
    /// Write an encrypted copy (AES-128, PDF 1.6 security handler) of the current
    /// state to `path`. The open document itself stays unencrypted in memory.
    pub fn save_encrypted(
        &self,
        path: impl AsRef<std::path::Path>,
        user_password: &str,
        owner_password: &str,
        permissions: SecurityPermissions,
    ) -> Result<()> {
        if user_password.is_empty() && owner_password.is_empty() {
            return Err(Error::Invalid("at least one password is required".into()));
        }
        let path = path.as_ref();
        let mut copy = lopdf::Document::load_mem(self.bytes().as_slice())?;
        if copy.trailer.get(b"ID").is_err() {
            let id = file_id();
            copy.trailer.set(
                "ID",
                Object::Array(vec![
                    Object::String(id.clone(), StringFormat::Hexadecimal),
                    Object::String(id, StringFormat::Hexadecimal),
                ]),
            );
        }
        let owner = if owner_password.is_empty() {
            user_password
        } else {
            owner_password
        };
        let mut filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
        filters.insert(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter));
        let state = EncryptionState::try_from(EncryptionVersion::V4 {
            document: &copy,
            encrypt_metadata: true,
            crypt_filters: filters,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: owner,
            user_password,
            permissions: permissions.to_lopdf(),
        })?;
        copy.encrypt(&state)?;
        copy.save(path)?;
        Ok(())
    }
}
