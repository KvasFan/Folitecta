//! Password protection: the profiles of new protection (AES-256, AES-128),
//! the protection a document carries, and protected copies.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, LazyLock};

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, Aes256CryptFilter, CryptFilter};
use lopdf::{
    Dictionary, EncryptionState, EncryptionVersion, Object, ObjectId, Permissions, StringFormat, dictionary,
};
use rand_core::{OsRng, RngCore};

use super::encrypted;
use crate::document::{Document, serialize};
use crate::error::{Error, Result};

/// What a reader opening the file with the user password may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
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
    /// The permission bits of the encryption dictionary. Printing is allowed
    /// at full quality; content stays available to assistive technology
    /// whatever `copy` says (PDF 2.0 asks that this bit is always set).
    pub(crate) fn to_lopdf(self) -> Permissions {
        let mut p = Permissions::COPYABLE_FOR_ACCESSIBILITY;
        if self.print {
            p |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
        }
        if self.modify {
            p |= Permissions::MODIFIABLE;
        }
        if self.copy {
            p |= Permissions::COPYABLE;
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

/// What [`Error::UnsupportedProtection`] says of a file protected by
/// certificates (public-key protection).
pub const CERTIFICATE_PROTECTION: &str = "certificates (public-key protection)";

/// The encryption of new protection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EncryptionProfile {
    /// AES with a 256-bit key (security handler revision 6, PDF 2.0).
    #[default]
    Aes256,
    /// AES with a 128-bit key (revision 4, PDF 1.6), for older readers.
    Aes128,
}

/// Why a password cannot be used with a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordIssue {
    /// It has characters the profile cannot keep.
    Characters,
    /// It is longer than the profile keeps.
    TooLong,
}

/// The characters of PDFDocEncoding, the only ones AES-128 (and RC4)
/// passwords can carry: built from the table the PDF library encodes
/// passwords with, which drops every other character.
static PDF_DOC_CHARS: LazyLock<HashSet<char>> =
    LazyLock::new(|| (1u8..=255).filter_map(pdf_doc_char).collect());

/// Every character of `text` has a PDFDocEncoding code.
pub fn fits_pdf_doc_encoding(text: &str) -> bool {
    text.chars().all(|c| PDF_DOC_CHARS.contains(&c))
}

/// The character of each PDFDocEncoding code.
fn pdf_doc_char(b: u8) -> Option<char> {
    let text = lopdf::decode_text_string(&Object::String(vec![b], StringFormat::Literal)).ok()?;
    let mut chars = text.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

/// The padding of passwords of revisions 2 to 4 (Algorithm 2).
const PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08, 0x2E,
    0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

fn md5(data: &[u8]) -> Vec<u8> {
    use md5::Digest as _;
    md5::Md5::digest(data).to_vec()
}

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    use lopdf::encryption::crypt_filters::Rc4CryptFilter;
    Rc4CryptFilter.decrypt(key, data).unwrap_or_default()
}

/// The bytes of `text` in PDFDocEncoding, as the PDF library encodes
/// passwords of revisions 2 to 4 (characters without a code are left out).
fn pdf_doc_bytes(text: &str) -> Vec<u8> {
    text.chars()
        .filter_map(|c| (1u8..=255).find(|b| pdf_doc_char(*b) == Some(c)))
        .collect()
}

/// The user password (32 bytes, padded) an owner password of revisions 2
/// to 4 stands for (Algorithm 7).
fn user_of_owner(dict: &lopdf::Dictionary, revision: i64, owner: &[u8]) -> Vec<u8> {
    let owner_value = dict.get(b"O").and_then(Object::as_str).unwrap_or_default();
    let version = dict.get(b"V").and_then(Object::as_i64).unwrap_or(0);
    // Version 4 fixes the key at 128 bits; /Length may be left out.
    let default_length = if version == 4 { 128 } else { 40 };
    let length = dict
        .get(b"Length")
        .and_then(Object::as_i64)
        .unwrap_or(default_length);
    let mut padded: Vec<u8> = owner.iter().copied().take(32).collect();
    padded.extend_from_slice(&PAD[..32 - padded.len()]);
    let mut hash = md5(&padded);
    let n = if revision >= 3 {
        for _ in 0..50 {
            hash = md5(&hash);
        }
        (length / 8).clamp(5, 16) as usize
    } else {
        5
    };
    let mut user = owner_value.to_vec();
    if revision >= 3 {
        for i in (1..=19u8).rev() {
            let key: Vec<u8> = hash[..n].iter().map(|b| b ^ i).collect();
            user = rc4(&key, &user);
        }
    }
    rc4(&hash[..n], &user)
}

/// How a file of revision 2 to 4 opened with a password is decrypted.
#[derive(Debug, PartialEq)]
pub(crate) enum R4Key {
    /// Not a file of revision 2 to 4 that needs its password (the PDF
    /// library's reading applies).
    NotR4,
    /// An ASCII user password: the PDF library makes the right key of it.
    Library,
    /// The password bytes that give the file key (decrypted here).
    Key(Vec<u8>),
    /// No form of the password opens the file.
    Wrong,
}

/// For an encrypted file of revision 2 to 4 opened with `password`: how to
/// decrypt it. The PDF library checks the password in PDFDocEncoding but
/// makes the key from its UTF-8 bytes (wrong for every character outside
/// ASCII), and makes it from an owner password as if it were the user
/// password; either way the file would be decrypted into noise, so those
/// passwords are decrypted here. The password is tried as typed and
/// composed (NFC: accents typed as separate marks are stored composed), in
/// PDFDocEncoding and as UTF-8 bytes (as some producers write it).
pub(crate) fn r4_key_password(bytes: &[u8], password: &str) -> R4Key {
    use unicode_normalization::UnicodeNormalization;
    if password.is_empty() {
        return R4Key::NotR4;
    }
    // Without a password the file stays encrypted (when it has a user password).
    let Ok(doc) = lopdf::Document::load_mem(bytes) else {
        return R4Key::NotR4;
    };
    let Ok(dict) = doc.get_encrypted() else {
        return R4Key::NotR4;
    };
    let revision = dict.get(b"R").and_then(Object::as_i64).unwrap_or(0);
    if !(2..=4).contains(&revision) || other_handler(dict) {
        return R4Key::NotR4;
    }
    let typed = password.as_bytes().to_vec();
    let composed: String = password.nfc().collect();
    let mut candidates: Vec<Vec<u8>> = Vec::new();
    for c in [
        pdf_doc_bytes(password),
        pdf_doc_bytes(&composed),
        typed.clone(),
        composed.as_bytes().to_vec(),
    ] {
        if !c.is_empty() && !candidates.contains(&c) {
            candidates.push(c);
        }
    }
    for candidate in candidates {
        if doc.authenticate_raw_user_password(&candidate).is_ok() {
            // The library's own reading is right only for an ASCII password.
            return if password.is_ascii() && candidate == typed {
                R4Key::Library
            } else {
                R4Key::Key(candidate)
            };
        }
        if doc.authenticate_raw_owner_password(&candidate).is_ok() {
            return R4Key::Key(user_of_owner(dict, revision, &candidate));
        }
    }
    R4Key::Wrong
}

/// The encryption dictionary names another handler than the standard one.
fn other_handler(dict: &lopdf::Dictionary) -> bool {
    dict.get(b"Filter")
        .and_then(Object::as_name)
        .is_ok_and(|f| f != b"Standard")
}

impl EncryptionProfile {
    pub const ALL: [EncryptionProfile; 2] = [EncryptionProfile::Aes256, EncryptionProfile::Aes128];

    /// The name kept in the settings and in policy files.
    pub fn key(self) -> &'static str {
        match self {
            EncryptionProfile::Aes256 => "aes-256",
            EncryptionProfile::Aes128 => "aes-128",
        }
    }

    /// The profile of a settings name; anything else than `aes-128` is AES-256.
    pub fn from_key(key: &str) -> Self {
        if key.trim().eq_ignore_ascii_case("aes-128") {
            EncryptionProfile::Aes128
        } else {
            EncryptionProfile::Aes256
        }
    }

    /// The technical name of the encryption (not translated).
    pub fn method(self) -> &'static str {
        match self {
            EncryptionProfile::Aes256 => "AES-256",
            EncryptionProfile::Aes128 => "AES-128",
        }
    }

    /// Whether `password` is kept whole by this profile. AES-128 keeps at
    /// most 32 characters of PDFDocEncoding; AES-256 keeps 127 bytes of the
    /// password prepared with SASLprep, which refuses some characters
    /// (controls, characters unknown to Unicode 3.2 such as emoji).
    pub fn check_password(self, password: &str) -> std::result::Result<(), PasswordIssue> {
        match self {
            EncryptionProfile::Aes128 => {
                if !fits_pdf_doc_encoding(password) {
                    Err(PasswordIssue::Characters)
                } else if password.chars().count() > 32 {
                    Err(PasswordIssue::TooLong)
                } else {
                    Ok(())
                }
            }
            EncryptionProfile::Aes256 => {
                let prepared = stringprep::saslprep(password).map_err(|_| PasswordIssue::Characters)?;
                if prepared.len() > 127 {
                    Err(PasswordIssue::TooLong)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// The profile keeps `password` whole (see [`EncryptionProfile::check_password`]).
    pub fn accepts_password(self, password: &str) -> bool {
        self.check_password(password).is_ok()
    }
}

impl serde::Serialize for EncryptionProfile {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.key())
    }
}

impl<'de> serde::Deserialize<'de> for EncryptionProfile {
    /// Lenient: an unknown name, or a value that is not a name, is AES-256
    /// (settings written by other versions never fail to load).
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Key(String),
            Other(serde::de::IgnoredAny),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::Key(key) => EncryptionProfile::from_key(&key),
            Raw::Other(_) => EncryptionProfile::default(),
        })
    }
}

/// The encryption of a protected document, in words and numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptionInfo {
    /// "AES-256", "AES-128", "RC4 128-bit", ...
    pub method: String,
    /// `/V` and `/R` of the encryption dictionary.
    pub version: i64,
    pub revision: i64,
    /// `/P`: what a reader opening with the user password may do.
    pub permissions: i64,
}

/// The protection of a document: the key, passwords and permissions of its
/// encryption, applied to every file written from it. Cheap to clone; the
/// key is never printed.
#[derive(Clone)]
pub struct Protection(Arc<ProtectionData>);

pub(crate) struct ProtectionData {
    state: EncryptionState,
    /// The trailer `/ID` of the file (the key of revisions 2 to 4 depends on it).
    id: Option<Object>,
    /// The encryption dictionary written into whole files: the one of the
    /// opened file as it was, or the one made for new protection.
    dict: Option<Dictionary>,
    /// Object number of the encryption dictionary in the opened file.
    encrypt_ref: Option<ObjectId>,
    /// Why files cannot be written with this protection (a feature of the
    /// opened file Folitecta cannot reproduce).
    unsupported: Option<&'static str>,
    /// The file was opened with its owner password, or protected in this
    /// session: every permission is granted (PDF 32000-1, 7.6.4).
    owner: bool,
}

impl PartialEq for Protection {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for Protection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Protection").field(&self.info().method).finish()
    }
}

/// A trailer `/ID`: two equal strings of 16 random bytes.
fn random_id() -> Object {
    let mut id = vec![0u8; 16];
    OsRng.fill_bytes(&mut id);
    Object::Array(vec![
        Object::String(id.clone(), StringFormat::Hexadecimal),
        Object::String(id, StringFormat::Hexadecimal),
    ])
}

impl Protection {
    /// The protection of a file that was opened: its decryption state, the
    /// `/ID` of its trailer, its encryption dictionary as written (when it
    /// could be read) and where that dictionary is.
    pub(crate) fn opened(
        state: EncryptionState,
        id: Option<Object>,
        dict: Option<Dictionary>,
        encrypt_ref: Option<ObjectId>,
    ) -> Self {
        let unsupported = dict.as_ref().and_then(unsupported_feature);
        let dict = dict.or_else(|| state.encode().ok());
        Protection(Arc::new(ProtectionData {
            state,
            id,
            dict,
            encrypt_ref,
            unsupported,
            owner: false,
        }))
    }

    /// The same protection, opened with the owner password.
    pub(crate) fn opened_by_owner(&self) -> Self {
        let d = &self.0;
        Protection(Arc::new(ProtectionData {
            state: d.state.clone(),
            id: d.id.clone(),
            dict: d.dict.clone(),
            encrypt_ref: d.encrypt_ref,
            unsupported: d.unsupported,
            owner: true,
        }))
    }

    /// The document was opened with the owner password of its file, or was
    /// protected in this session: the permissions do not restrict it. Opened
    /// with the user password (or none), the permissions of the file apply.
    pub fn owner_access(&self) -> bool {
        self.0.owner
    }

    /// `password` (empty: none) is the owner password of this protection,
    /// checked against its encryption dictionary (never the whole file
    /// again). Revisions 2 to 4 take it in PDFDocEncoding when every
    /// character has a code there, else as typed; AES-256 prepared with
    /// SASLprep, else as typed. A character dropped on the way never makes
    /// another password match.
    pub fn is_owner_password(&self, password: &str) -> bool {
        let Some(dict) = self.0.dict.clone() else {
            return false;
        };
        let revision = dict.get(b"R").and_then(Object::as_i64).unwrap_or(0);
        let mut doc = lopdf::Document::with_version("1.7");
        let encrypt = doc.add_object(dict);
        doc.trailer.set("Encrypt", Object::Reference(encrypt));
        if let Some(id) = &self.0.id {
            doc.trailer.set("ID", id.clone());
        }
        let mut candidates: Vec<Vec<u8>> = Vec::new();
        if revision <= 4 {
            if fits_pdf_doc_encoding(password) {
                candidates.push(pdf_doc_bytes(password));
            }
        } else if let Ok(prepared) = stringprep::saslprep(password) {
            candidates.push(prepared.as_bytes().to_vec());
        }
        candidates.push(password.as_bytes().to_vec());
        candidates.dedup();
        candidates
            .iter()
            .any(|c| doc.authenticate_raw_owner_password(c).is_ok())
    }

    /// This protection with the owner's access, when `password` is its owner
    /// password.
    pub fn unlocked(&self, password: &str) -> Option<Protection> {
        if self.owner_access() {
            return Some(self.clone());
        }
        self.is_owner_password(password).then(|| self.opened_by_owner())
    }

    /// New protection of `doc` with the passwords, permissions and profile
    /// given (the owner password is the user password when it is empty).
    /// Gives `doc` an `/ID` when it has none.
    pub(crate) fn new(
        user: &str,
        owner: &str,
        permissions: SecurityPermissions,
        profile: EncryptionProfile,
        doc: &mut lopdf::Document,
    ) -> Result<Self> {
        if user.is_empty() && owner.is_empty() {
            return Err(Error::Invalid("at least one password is required".into()));
        }
        let owner = if owner.is_empty() { user } else { owner };
        for pw in [user, owner] {
            match profile.check_password(pw) {
                Ok(()) => {}
                Err(PasswordIssue::Characters) => {
                    return Err(Error::Invalid(format!(
                        "the passwords have characters {} protection cannot keep; nothing was written",
                        profile.method()
                    )));
                }
                Err(PasswordIssue::TooLong) => {
                    return Err(Error::Invalid(format!(
                        "the passwords are longer than {} protection keeps; nothing was written",
                        profile.method()
                    )));
                }
            }
        }
        if doc.trailer.get(b"ID").is_err() {
            doc.trailer.set("ID", random_id());
        }
        let refused = |_| Error::Invalid("the passwords cannot be used for this protection".into());
        let p = permissions.to_lopdf();
        let mut filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
        let (state, cf_length) = match profile {
            EncryptionProfile::Aes128 => {
                filters.insert(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter));
                let state = EncryptionState::try_from(EncryptionVersion::V4 {
                    document: doc,
                    encrypt_metadata: true,
                    crypt_filters: filters,
                    stream_filter: b"StdCF".to_vec(),
                    string_filter: b"StdCF".to_vec(),
                    owner_password: owner,
                    user_password: user,
                    permissions: p,
                })
                .map_err(refused)?;
                (state, 16)
            }
            EncryptionProfile::Aes256 => {
                filters.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));
                let mut key = [0u8; 32];
                OsRng.fill_bytes(&mut key);
                let state = EncryptionState::try_from(EncryptionVersion::V5 {
                    encrypt_metadata: true,
                    crypt_filters: filters,
                    file_encryption_key: &key,
                    stream_filter: b"StdCF".to_vec(),
                    string_filter: b"StdCF".to_vec(),
                    owner_password: owner,
                    user_password: user,
                    permissions: p,
                })
                .map_err(refused)?;
                (state, 32)
            }
        };
        let mut dict = state
            .encode()
            .map_err(|e| Error::ProtectionNotKept(e.to_string()))?;
        if profile == EncryptionProfile::Aes256 {
            dict.set("Length", 256);
        }
        // The crypt filter says its key length (in bytes) and that it applies
        // when the document is opened, as readers expect.
        if let Ok(Object::Dictionary(cf)) = dict.get_mut(b"CF") {
            if let Ok(Object::Dictionary(std_cf)) = cf.get_mut(b"StdCF") {
                std_cf.set("AuthEvent", Object::Name(b"DocOpen".to_vec()));
                std_cf.set("Length", cf_length);
            }
        }
        Ok(Protection(Arc::new(ProtectionData {
            state,
            id: doc.trailer.get(b"ID").ok().cloned(),
            dict: Some(dict),
            encrypt_ref: None,
            unsupported: None,
            // The passwords were set here: the owner's is known.
            owner: true,
        })))
    }

    pub(crate) fn state(&self) -> &EncryptionState {
        &self.0.state
    }

    pub(crate) fn id(&self) -> Option<&Object> {
        self.0.id.as_ref()
    }

    /// Where the encryption dictionary of the opened file is.
    pub(crate) fn encrypt_ref(&self) -> Option<ObjectId> {
        self.0.encrypt_ref
    }

    /// The encryption dictionary to write, or why the protection cannot be
    /// written. Its binary strings are written in hexadecimal (in a literal
    /// string, readers turn the line ends among them into other bytes).
    pub(crate) fn dictionary(&self) -> Result<Dictionary> {
        self.writable()?;
        let mut dict =
            self.0.dict.clone().ok_or_else(|| {
                Error::ProtectionNotKept("its encryption dictionary could not be read".into())
            })?;
        for key in [&b"O"[..], b"U", b"OE", b"UE", b"Perms"] {
            if let Ok(Object::String(_, format)) = dict.get_mut(key) {
                *format = StringFormat::Hexadecimal;
            }
        }
        Ok(dict)
    }

    /// Files can be written with this protection.
    pub(crate) fn writable(&self) -> Result<()> {
        match self.0.unsupported {
            Some(why) => Err(Error::ProtectionNotKept(why.into())),
            None => Ok(()),
        }
    }

    /// The encryption in words and numbers.
    pub fn info(&self) -> EncryptionInfo {
        let s = &self.0.state;
        let cfm = |name: &[u8]| s.crypt_filters().get(name).map(|f| f.method().to_vec());
        let method = match (s.version(), s.revision()) {
            (5, 5) => "AES-256 (R5)".to_string(),
            (5, _) => "AES-256".to_string(),
            (4, _) => match cfm(s.default_stream_filter()).as_deref() {
                Some(b"AESV2") => "AES-128".to_string(),
                Some(b"AESV3") => "AES-256".to_string(),
                _ => "RC4 128-bit".to_string(),
            },
            (1, _) => "RC4 40-bit".to_string(),
            (2 | 3, _) => format!("RC4 {}-bit", s.key_length().unwrap_or(40)),
            (v, r) => format!("V{v} R{r}"),
        };
        EncryptionInfo {
            method,
            version: s.version(),
            revision: s.revision(),
            permissions: s.permissions().bits() as i64,
        }
    }
}

/// A feature of an opened file's standard encryption that Folitecta cannot
/// write back the same way: files are then not written with it.
fn unsupported_feature(d: &Dictionary) -> Option<&'static str> {
    let v = d.get(b"V").and_then(Object::as_i64).unwrap_or(0);
    if v < 4 {
        return None;
    }
    let name = |key: &[u8]| d.get(key).and_then(Object::as_name).ok();
    let cf = d.get(b"CF").and_then(Object::as_dict).ok();
    let known = |filter: &[u8]| {
        cf.and_then(|c| c.get(filter).ok())
            .and_then(|f| f.as_dict().ok())
            .is_some_and(|f| match f.get(b"CFM").and_then(Object::as_name) {
                Ok(m) => matches!(m, b"V2" | b"AESV2" | b"AESV3"),
                Err(_) => true,
            })
    };
    let stream = name(b"StmF").unwrap_or(b"Identity");
    let string = name(b"StrF").unwrap_or(b"Identity");
    if stream == b"Identity" || string == b"Identity" {
        return Some(
            "the file leaves its streams or strings unencrypted, which Folitecta cannot write the same way",
        );
    }
    if !known(stream) || !known(string) {
        return Some("the file uses an encryption method Folitecta does not know");
    }
    if name(b"EFF").is_some_and(|eff| eff != stream) {
        return Some(
            "the file encrypts its attachments separately, which Folitecta cannot write the same way",
        );
    }
    None
}

/// `major.minor` of a version string ("1.7" → (1, 7)).
fn version_number(v: &str) -> (u32, u32) {
    let mut parts = v.trim().split('.');
    let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(1);
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    (major, minor)
}

/// Raise the version of `doc` to the one the profile needs: 1.6 for AES-128;
/// 1.7 with the extension level 8 of the `ADBE` developer prefix for AES-256
/// below PDF 2.0 (the level that defined revision 6, as widely deployed
/// writers declare it).
fn declare_profile(doc: &mut lopdf::Document, profile: EncryptionProfile) -> Result<()> {
    let current = version_number(&doc.version);
    match profile {
        EncryptionProfile::Aes128 => {
            if current < (1, 6) {
                doc.version = "1.6".into();
            }
            Ok(())
        }
        EncryptionProfile::Aes256 => {
            if current >= (2, 0) {
                return Ok(());
            }
            if current < (1, 7) {
                doc.version = "1.7".into();
            }
            declare_extension_level_8(doc)
        }
    }
}

fn declare_extension_level_8(doc: &mut lopdf::Document) -> Result<()> {
    let level_ok = |doc: &lopdf::Document, o: &Object| {
        crate::util::deref(doc, o)
            .as_dict()
            .ok()
            .and_then(|d| d.get(b"ExtensionLevel").ok())
            .and_then(|l| l.as_i64().ok())
            .is_some_and(|l| l >= 8)
    };
    let extension = || {
        Object::Dictionary(dictionary! {
            "BaseVersion" => Object::Name(b"1.7".to_vec()),
            "ExtensionLevel" => 8,
        })
    };
    let root = doc.trailer.get(b"Root").and_then(Object::as_reference)?;
    let current = doc.get_dictionary(root)?.get(b"Extensions").ok().cloned();
    // The extensions dictionary, and where it lives (the catalog or its own object).
    let (mut ext, holder) = match current {
        Some(Object::Reference(id)) => (doc.get_dictionary(id)?.clone(), Some(id)),
        Some(Object::Dictionary(d)) => (d, None),
        _ => (Dictionary::new(), None),
    };
    let adbe = ext.get(b"ADBE").ok().cloned();
    let resolved = adbe.as_ref().map(|a| crate::util::deref(doc, a).clone());
    match resolved {
        Some(Object::Array(items)) => {
            if !items.iter().any(|i| level_ok(doc, i)) {
                let mut items = items;
                items.push(extension());
                ext.set("ADBE", Object::Array(items));
            }
        }
        Some(o) if level_ok(doc, &o) => {}
        _ => ext.set("ADBE", extension()),
    }
    match holder {
        Some(id) => {
            doc.objects.insert(id, Object::Dictionary(ext));
        }
        None => {
            doc.get_dictionary_mut(root)?
                .set("Extensions", Object::Dictionary(ext));
        }
    }
    Ok(())
}

/// The bytes of a copy of the whole PDF `plain` protected with new
/// passwords, permissions and profile (the copy is checked to carry its
/// encryption dictionary).
pub fn protected_copy(
    plain: &[u8],
    user_password: &str,
    owner_password: &str,
    permissions: SecurityPermissions,
    profile: EncryptionProfile,
) -> Result<Vec<u8>> {
    let mut copy = lopdf::Document::load_mem(plain)?;
    if copy.is_encrypted() {
        return Err(Error::Invalid("the copy to protect is already encrypted".into()));
    }
    let protection = Protection::new(user_password, owner_password, permissions, profile, &mut copy)?;
    declare_profile(&mut copy, profile)?;
    encrypted::encrypt_document(&mut copy, &protection)?;
    let out = serialize(&mut copy)?;
    encrypted::check_written(&out)?;
    Ok(out)
}

impl Document {
    /// Write a copy of the current state protected with new passwords,
    /// permissions and encryption profile to `path`. The open document
    /// itself is not changed (and stays decrypted in memory).
    pub fn save_encrypted(
        &self,
        path: impl AsRef<std::path::Path>,
        user_password: &str,
        owner_password: &str,
        permissions: SecurityPermissions,
        profile: EncryptionProfile,
    ) -> Result<()> {
        let out = protected_copy(
            self.bytes().as_slice(),
            user_password,
            owner_password,
            permissions,
            profile,
        )?;
        crate::document::write_pdf_file(path.as_ref(), &out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_doc_encoding_fits() {
        assert!(fits_pdf_doc_encoding("pässwörd€"));
        assert!(fits_pdf_doc_encoding("Łódka"));
        assert!(!fits_pdf_doc_encoding("łódź"));
        assert!(!fits_pdf_doc_encoding("密码"));
    }

    #[test]
    fn profiles_check_their_passwords() {
        use EncryptionProfile::*;
        assert_eq!(Aes128.check_password("密码"), Err(PasswordIssue::Characters));
        assert_eq!(
            Aes128.check_password(&"a".repeat(33)),
            Err(PasswordIssue::TooLong)
        );
        assert!(Aes128.accepts_password(&"a".repeat(32)));
        assert!(Aes256.accepts_password("密码"));
        // 43 CJK characters are 129 bytes of UTF-8.
        assert_eq!(
            Aes256.check_password(&"密".repeat(43)),
            Err(PasswordIssue::TooLong)
        );
        assert!(Aes256.accepts_password(&"密".repeat(42)));
        // SASLprep refuses controls and characters unknown to Unicode 3.2.
        assert_eq!(Aes256.check_password("a\tb"), Err(PasswordIssue::Characters));
        assert_eq!(Aes256.check_password("pw🙂"), Err(PasswordIssue::Characters));
        // and maps others (a no-break space is a space).
        assert!(Aes256.accepts_password("a\u{a0}b"));
    }

    #[test]
    fn permission_bits() {
        let p = |perms: SecurityPermissions| {
            let mut doc = lopdf::Document::with_version("1.7");
            Protection::new("u", "o", perms, EncryptionProfile::Aes128, &mut doc)
                .unwrap()
                .info()
                .permissions
        };
        assert_eq!(p(SecurityPermissions::default()), -4);
        assert_eq!(
            p(SecurityPermissions {
                print: false,
                ..Default::default()
            }),
            -2056
        );
        assert_eq!(
            p(SecurityPermissions {
                copy: false,
                ..Default::default()
            }),
            -20
        );
    }

    #[test]
    fn profile_names_load_leniently() {
        #[derive(serde::Deserialize)]
        struct S {
            #[serde(default)]
            e: EncryptionProfile,
        }
        let e = |json: &str| serde_json::from_str::<S>(json).unwrap().e;
        assert_eq!(e("{}"), EncryptionProfile::Aes256);
        assert_eq!(e(r#"{"e":"aes-128"}"#), EncryptionProfile::Aes128);
        assert_eq!(e(r#"{"e":"aes-512"}"#), EncryptionProfile::Aes256);
        assert_eq!(e(r#"{"e":null}"#), EncryptionProfile::Aes256);
        assert_eq!(e(r#"{"e":128}"#), EncryptionProfile::Aes256);
        assert_eq!(
            serde_json::to_string(&EncryptionProfile::Aes128).unwrap(),
            r#""aes-128""#
        );
    }
}

/// Files protected by revisions 2 to 4 written here byte by byte, as other
/// producers write them (the PDF library writes neither encrypted object
/// streams nor passwords made from UTF-8 bytes).
#[cfg(test)]
mod handmade {
    use std::collections::BTreeMap;

    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter, Rc4CryptFilter};

    use super::*;
    use crate::document::Document;

    const FILE_ID: [u8; 16] = [5; 16];

    fn padded(pw: &[u8]) -> Vec<u8> {
        let mut v: Vec<u8> = pw.iter().copied().take(32).collect();
        v.extend_from_slice(&PAD[..32 - v.len()]);
        v
    }

    fn hex(data: &[u8]) -> String {
        data.iter().map(|b| format!("{b:02X}")).collect()
    }

    /// The standard security handler of revision `r` (2, 3: RC4; 4: RC4 or
    /// AES-128) with passwords given as the bytes the producer used.
    struct Sec {
        r: i64,
        aes: bool,
        key: Vec<u8>,
        o: Vec<u8>,
        u: Vec<u8>,
    }

    impl Sec {
        fn new(r: i64, user: &[u8], owner: &[u8], aes: bool) -> Sec {
            let n = if r == 2 { 5 } else { 16 };
            // Algorithm 3: O.
            let mut h = md5(&padded(if owner.is_empty() { user } else { owner }));
            if r >= 3 {
                for _ in 0..50 {
                    h = md5(&h);
                }
            }
            let k = h[..n].to_vec();
            let mut o = rc4(&k, &padded(user));
            if r >= 3 {
                for i in 1..=19u8 {
                    o = rc4(&k.iter().map(|b| b ^ i).collect::<Vec<_>>(), &o);
                }
            }
            // Algorithm 2: the key.
            let mut m = padded(user);
            m.extend_from_slice(&o);
            m.extend_from_slice(&(-4i32 as u32).to_le_bytes());
            m.extend_from_slice(&FILE_ID);
            let mut h = md5(&m);
            if r >= 3 {
                for _ in 0..50 {
                    h = md5(&h[..n]);
                }
            }
            let key = h[..n].to_vec();
            // Algorithms 4 and 5: U.
            let u = if r == 2 {
                rc4(&key, &PAD)
            } else {
                let mut seed = PAD.to_vec();
                seed.extend_from_slice(&FILE_ID);
                let mut u = rc4(&key, &md5(&seed));
                for i in 1..=19u8 {
                    u = rc4(&key.iter().map(|b| b ^ i).collect::<Vec<_>>(), &u);
                }
                u.extend_from_slice(&[0; 16]);
                u
            };
            Sec { r, aes, key, o, u }
        }

        fn crypt(&self, id: u32, data: &[u8]) -> Vec<u8> {
            let filter: &dyn CryptFilter = if self.aes {
                &Aes128CryptFilter
            } else {
                &Rc4CryptFilter
            };
            let key = filter.compute_key(&self.key, (id, 0)).unwrap();
            filter.encrypt(&key, data).unwrap()
        }

        fn dictionary(&self) -> String {
            let (o, u) = (hex(&self.o), hex(&self.u));
            match self.r {
                4 => format!(
                    "<< /Filter /Standard /V 4 /R 4 /Length 128 /O <{o}> /U <{u}> /P -4 /CF << /StdCF << /Type /CryptFilter /CFM /{} /AuthEvent /DocOpen /Length 16 >> >> /StmF /StdCF /StrF /StdCF >>",
                    if self.aes { "AESV2" } else { "V2" }
                ),
                r => format!(
                    "<< /Filter /Standard /V {} /R {r} /Length {} /O <{o}> /U <{u}> /P -4 >>",
                    if r == 2 { 1 } else { 2 },
                    if r == 2 { 40 } else { 128 }
                ),
            }
        }
    }

    struct Out {
        bytes: Vec<u8>,
        offsets: BTreeMap<u32, usize>,
    }

    impl Out {
        fn obj(&mut self, n: u32, text: &str) {
            self.offsets.insert(n, self.bytes.len());
            self.bytes.extend(format!("{n} 0 obj\n{text}\nendobj\n").bytes());
        }

        fn stream(&mut self, n: u32, dict: &str, data: &[u8]) {
            self.offsets.insert(n, self.bytes.len());
            self.bytes
                .extend(format!("{n} 0 obj\n<< {dict} /Length {} >>\nstream\n", data.len()).bytes());
            self.bytes.extend_from_slice(data);
            self.bytes.extend(b"\nendstream\nendobj\n");
        }

        /// A cross-reference stream `n` of `entries` (number, type, field 2, field 3).
        fn xref(
            &mut self,
            n: u32,
            mut entries: Vec<(u32, u8, u32, u16)>,
            trailer: &str,
            prev: Option<usize>,
        ) -> usize {
            let at = self.bytes.len();
            entries.push((n, 1, at as u32, 0));
            entries.sort();
            let size = entries.iter().map(|e| e.0).max().unwrap_or(0) + 1;
            let mut data = Vec::new();
            let mut index = String::new();
            for (num, kind, f2, f3) in &entries {
                data.push(*kind);
                data.extend(f2.to_be_bytes());
                data.extend(f3.to_be_bytes());
                index.push_str(&format!("{num} 1 "));
            }
            let prev = prev.map(|p| format!("/Prev {p}")).unwrap_or_default();
            self.bytes.extend(
                format!(
                    "{n} 0 obj\n<< /Type /XRef /Size {size} /W [1 4 2] /Index [{index}] {trailer} {prev} /Length {} >>\nstream\n",
                    data.len()
                )
                .bytes(),
            );
            self.bytes.extend(data);
            self.bytes
                .extend(format!("\nendstream\nendobj\nstartxref\n{at}\n%%EOF\n").bytes());
            at
        }
    }

    fn object_stream(members: &[(u32, &str)]) -> (String, Vec<u8>) {
        let (mut head, mut body) = (String::new(), String::new());
        for (n, text) in members {
            head.push_str(&format!("{n} {} ", body.len()));
            body.push_str(text);
            body.push('\n');
        }
        (
            format!("/Type /ObjStm /N {} /First {}", members.len(), head.len() + 1),
            format!("{head}\n{body}").into_bytes(),
        )
    }

    /// Two revisions: the page tree and object 6 (`/Val (OLD-VALUE)`) and 7
    /// live in object stream 10; the second revision puts a new object 6
    /// (`/Val (NEW-VALUE)`) in object stream 20 and frees object 7, while
    /// stream 10 stays.
    fn two_revisions(r: i64, user: &[u8], owner: &[u8], aes: bool) -> Vec<u8> {
        let sec = Sec::new(r, user, owner, aes);
        let mut out = Out {
            bytes: b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec(),
            offsets: BTreeMap::new(),
        };
        out.obj(1, "<< /Type /Catalog /Pages 2 0 R /Probe 6 0 R >>");
        let members = [
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
            ),
            (6, "<< /Val (OLD-VALUE) >>"),
            (7, "<< /Zombie (ZOMBIE) >>"),
        ];
        let (dict, data) = object_stream(&members);
        out.stream(10, &dict, &sec.crypt(10, &data));
        out.obj(4, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
        out.stream(
            5,
            "",
            &sec.crypt(5, b"BT /F1 12 Tf 20 100 Td (MARK-OBJSTM) Tj ET"),
        );
        out.obj(8, &sec.dictionary());
        let mut entries = vec![(0, 0, 0, 0xFFFF)];
        for n in [1, 4, 5, 8, 10] {
            entries.push((n, 1, out.offsets[&n] as u32, 0));
        }
        for (i, (n, _)) in members.iter().enumerate() {
            entries.push((*n, 2, 10, i as u16));
        }
        let trailer = format!("/Root 1 0 R /Encrypt 8 0 R /ID [<{0}> <{0}>]", hex(&FILE_ID));
        let first = out.xref(9, entries, &trailer, None);
        let (dict, data) = object_stream(&[(6, "<< /Val (NEW-VALUE) >>")]);
        out.stream(20, &dict, &sec.crypt(20, &data));
        let entries = vec![(6, 2, 20, 0), (7, 0, 0, 1), (20, 1, out.offsets[&20] as u32, 0)];
        out.xref(21, entries, &trailer, Some(first));
        out.bytes
    }

    fn val(doc: &Document) -> Vec<u8> {
        doc.inner
            .get_dictionary((6, 0))
            .and_then(|d| d.get(b"Val"))
            .and_then(Object::as_str)
            .map(|s| s.to_vec())
            .unwrap_or_default()
    }

    /// The objects of a document, comparable whatever the reading.
    fn objects(doc: &Document) -> BTreeMap<lopdf::ObjectId, Object> {
        doc.inner
            .objects
            .iter()
            .filter(|(_, o)| !matches!(o.type_name(), Ok(b"XRef")))
            .map(|(id, o)| {
                let mut o = o.clone();
                if let Object::Stream(s) = &mut o {
                    s.start_position = None;
                }
                (*id, o)
            })
            .collect()
    }

    #[test]
    fn passwords_typed_composed_or_decomposed_open_the_content() {
        // Accents typed as separate marks (NFD) are stored composed (NFC)
        // in PDFDocEncoding: both forms open the file, with its content.
        for (r, aes) in [(2, false), (3, false), (4, false), (4, true)] {
            let raw = two_revisions(r, &pdf_doc_bytes("pässwörd"), &pdf_doc_bytes("Ownér€"), aes);
            for typed in ["pässwörd", "pa\u{308}sswo\u{308}rd", "Ownér€", "Owne\u{301}r€"] {
                let doc = Document::from_bytes(raw.clone(), None, Some(typed))
                    .unwrap_or_else(|e| panic!("R{r} aes {aes}, {typed:?}: {e}"));
                assert_eq!(val(&doc), b"NEW-VALUE", "R{r} aes {aes}, {typed:?}");
                assert!(
                    doc.page_text(0).contains("MARK-OBJSTM"),
                    "R{r} aes {aes}, {typed:?}"
                );
            }
            for wrong in ["passwörd", "Owne\u{300}r€", "pässwörd ", "wrong"] {
                assert!(
                    matches!(
                        Document::from_bytes(raw.clone(), None, Some(wrong)),
                        Err(Error::WrongPassword)
                    ),
                    "R{r} aes {aes}, {wrong:?}"
                );
            }
        }
    }

    #[test]
    fn keys_are_used_only_once_verified() {
        // Bytes that are not the user password give no document (never a
        // document decrypted into noise).
        let raw = two_revisions(4, b"user", b"owner", true);
        for bytes in [&b"wrong"[..], b"owner", b""] {
            assert!(matches!(
                crate::document::load_decrypted_for_test(&raw, bytes),
                Err(Error::WrongPassword)
            ));
        }
        assert!(crate::document::load_decrypted_for_test(&raw, b"user").is_ok());
    }

    #[test]
    fn object_streams_of_later_revisions_win_whatever_the_password() {
        // ASCII user passwords are read by the PDF library; owner passwords
        // and passwords outside ASCII (PDFDocEncoding or UTF-8 bytes) here.
        // Every reading gives the same objects, with the newer object 6.
        let latin = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
        // Revision, AES, the user and owner passwords as written by the
        // producer, and as typed.
        type Case<'a> = (i64, bool, Vec<u8>, Vec<u8>, &'a str, &'a str);
        let cases: Vec<Case> = vec![
            (4, true, b"user".to_vec(), b"owner".to_vec(), "user", "owner"),
            (3, false, b"user".to_vec(), b"owner".to_vec(), "user", "owner"),
            (2, false, b"user".to_vec(), b"owner".to_vec(), "user", "owner"),
            (4, true, latin("pässword"), latin("Ownér"), "pässword", "Ownér"),
            (3, false, latin("pässword"), latin("Ownér"), "pässword", "Ownér"),
            (
                4,
                true,
                "пароль".as_bytes().to_vec(),
                "владелец".as_bytes().to_vec(),
                "пароль",
                "владелец",
            ),
            (
                3,
                false,
                "pässword".as_bytes().to_vec(),
                "ownér".as_bytes().to_vec(),
                "pässword",
                "ownér",
            ),
        ];
        for (r, aes, user, owner, typed_user, typed_owner) in cases {
            let raw = two_revisions(r, &user, &owner, aes);
            let mut reference = None;
            for pw in [typed_user, typed_owner] {
                let mut doc = Document::from_bytes(raw.clone(), None, Some(pw)).unwrap();
                let case = format!("R{r} aes {aes}, password {pw:?}");
                assert_eq!(val(&doc), b"NEW-VALUE", "{case}");
                assert!(doc.page_text(0).contains("MARK-OBJSTM"), "{case}");
                let objects = objects(&doc);
                match &reference {
                    None => reference = Some(objects),
                    Some(r) => assert!(*r == objects, "{case}: other objects than with the user password"),
                }
                // Saved and opened again: still the newer value.
                doc.rotate_pages(&[0], 90).unwrap();
                let file = doc.file_bytes_from().unwrap();
                for again in [typed_user, typed_owner] {
                    let back = Document::from_bytes(file.bytes.clone(), None, Some(again)).unwrap();
                    assert_eq!(val(&back), b"NEW-VALUE", "{case}, saved, opened with {again:?}");
                    assert!(back.page_text(0).contains("MARK-OBJSTM"), "{case}");
                }
            }
            assert!(matches!(
                Document::from_bytes(raw, None, Some("wrong")),
                Err(Error::WrongPassword)
            ));
        }
    }
}
