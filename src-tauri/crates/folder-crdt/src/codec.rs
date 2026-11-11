//! Turning a store's data into the bytes of its file and back.
//!
//! Every file is one JSON object: the cleartext [`Header`], any cleartext
//! parameters the cipher needs to open it again (a KDF salt and cost, say),
//! and the payload. [`Json`] stores the payload as plain JSON under `data`,
//! for data that isn't secret; [`Sealed`] encrypts it with an app-supplied
//! [`Cipher`] and stores it, base64-encoded, under `ciphertext`. With a
//! cipher, nothing but the header and its parameters reaches disk: no record
//! count, ids or timestamps.

use std::fmt;
use std::io;
use std::marker::PhantomData;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use crate::format::{FormatError, Header, MAGIC};

/// Why a store's file couldn't be read or written.
#[derive(Debug)]
pub enum StoreError {
    /// The file doesn't open with this key: most likely the password was
    /// changed on another device. A file in this state is never overwritten.
    KeyMismatch,
    /// Written by a newer build, by another app, or not a folder-crdt file.
    Format(FormatError),
    /// Damaged: not the JSON, base64 or record layout it should be.
    Corrupt(String),
    Io(io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::KeyMismatch => f.write_str("the file doesn't open with this key"),
            StoreError::Format(e) => e.fmt(f),
            StoreError::Corrupt(why) => write!(f, "the file is damaged: {why}"),
            StoreError::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<FormatError> for StoreError {
    fn from(e: FormatError) -> Self {
        StoreError::Format(e)
    }
}

/// [`Cipher::open`] couldn't open the payload with this key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyMismatch;

impl From<KeyMismatch> for StoreError {
    fn from(_: KeyMismatch) -> Self {
        StoreError::KeyMismatch
    }
}

/// Authenticated encryption, supplied by the app: folder-crdt has no crypto
/// of its own. `open` must fail, not return garbage, for a wrong key or a
/// tampered payload (AES-GCM and ChaCha20-Poly1305 both do).
pub trait Cipher {
    /// Cleartext values stored beside the payload that are needed to open it
    /// again, such as a KDF salt and cost. Never anything secret.
    fn params(&self) -> Map<String, Value> {
        Map::new()
    }

    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, StoreError>;

    fn open(&self, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, KeyMismatch>;
}

/// Turns a store's data into the bytes of its file and back.
pub trait Codec {
    type Data;

    fn encode(&self, data: &Self::Data) -> Result<Vec<u8>, StoreError>;

    /// Must never panic or allocate much beyond `bytes.len()`, whatever the
    /// bytes: a sync folder is untrusted storage.
    fn decode(&self, bytes: &[u8]) -> Result<Self::Data, StoreError>;
}

/// Reads a file written by an older schema of the app: given the schema the
/// file was written at and its payload as JSON, returns the data at the
/// current schema.
pub type Upgrade<T> = fn(schema: u32, payload: &[u8]) -> Result<T, String>;

fn corrupt(e: impl fmt::Display) -> StoreError {
    StoreError::Corrupt(e.to_string())
}

/// A file split into its parts, before the payload is opened.
#[derive(Debug, Clone)]
pub struct Envelope {
    /// Absent in files written before an app adopted folder-crdt's format.
    pub header: Option<Header>,
    /// Every other cleartext field: the cipher's parameters, and anything
    /// an app writes for its older builds to read.
    pub params: Map<String, Value>,
    /// The payload: decoded from base64 under `ciphertext`, or the JSON
    /// text under `data`.
    pub payload: Zeroizing<Vec<u8>>,
}

impl Envelope {
    pub fn parse(bytes: &[u8]) -> Result<Self, StoreError> {
        let mut params: Map<String, Value> = serde_json::from_slice(bytes).map_err(corrupt)?;
        let header = if params.contains_key("magic") {
            let mut fields = Map::new();
            for name in ["magic", "format", "app", "schema"] {
                if let Some(v) = params.remove(name) {
                    fields.insert(name.into(), v);
                }
            }
            let header: Header = serde_json::from_value(Value::Object(fields)).map_err(corrupt)?;
            if header.magic != MAGIC {
                return Err(FormatError::NotOurs.into());
            }
            Some(header)
        } else {
            None
        };
        let payload = match (params.remove("ciphertext"), params.remove("data")) {
            (Some(Value::String(mut text)), None) => {
                let bytes = B64.decode(&text).map_err(corrupt);
                zeroize::Zeroize::zeroize(&mut text);
                Zeroizing::new(bytes?)
            }
            (None, Some(data)) => Zeroizing::new(serde_json::to_vec(&data).map_err(corrupt)?),
            _ => return Err(StoreError::Corrupt("no payload".into())),
        };
        Ok(Envelope { header, params, payload })
    }

    /// Checks the header against this build's app and schema. A file without
    /// one is refused as not ours.
    pub fn check(&self, app: &str, schema: u32) -> Result<&Header, StoreError> {
        let header = self.header.as_ref().ok_or(FormatError::NotOurs)?;
        header.check(app, schema)?;
        Ok(header)
    }
}

/// The cleartext fields of a file, without opening its payload: what an app
/// reads to derive its key before it can build a [`Cipher`].
pub fn read_params(bytes: &[u8]) -> Result<Map<String, Value>, StoreError> {
    Ok(Envelope::parse(bytes)?.params)
}

/// Settings shared by both codecs.
struct Schema<T> {
    app: String,
    schema: u32,
    upgrade: Option<Upgrade<T>>,
    extra: Map<String, Value>,
}

impl<T: DeserializeOwned> Schema<T> {
    fn new(app: &str, schema: u32) -> Self {
        Schema { app: app.into(), schema, upgrade: None, extra: Map::new() }
    }

    fn file(&self, params: Map<String, Value>, payload_field: &str, payload: Value) -> Result<Vec<u8>, StoreError> {
        let mut file = self.extra.clone();
        file.extend(params);
        let header = serde_json::to_value(Header::new(&self.app, self.schema)).map_err(corrupt)?;
        if let Value::Object(header) = header {
            file.extend(header);
        }
        file.insert(payload_field.into(), payload);
        serde_json::to_vec(&file).map_err(corrupt)
    }

    fn read(&self, envelope: &Envelope, plaintext: &[u8]) -> Result<T, StoreError> {
        let header = envelope.check(&self.app, self.schema)?;
        match self.upgrade {
            Some(upgrade) if header.schema < self.schema => {
                upgrade(header.schema, plaintext).map_err(StoreError::Corrupt)
            }
            _ => serde_json::from_slice(plaintext).map_err(corrupt),
        }
    }
}

/// Stores the data as plain JSON under `data`. For data that isn't secret,
/// and for tests.
pub struct Json<T> {
    schema: Schema<T>,
    _data: PhantomData<fn() -> T>,
}

impl<T: Serialize + DeserializeOwned> Json<T> {
    /// A codec for `app`'s files at `schema`.
    pub fn new(app: &str, schema: u32) -> Self {
        Json { schema: Schema::new(app, schema), _data: PhantomData }
    }

    /// Reads files from older schemas with `upgrade`. Without one, they are
    /// read as if current.
    pub fn upgrade(mut self, upgrade: Upgrade<T>) -> Self {
        self.schema.upgrade = Some(upgrade);
        self
    }
}

impl<T: Serialize + DeserializeOwned> Codec for Json<T> {
    type Data = T;

    fn encode(&self, data: &T) -> Result<Vec<u8>, StoreError> {
        let value = serde_json::to_value(data).map_err(corrupt)?;
        self.schema.file(Map::new(), "data", value)
    }

    fn decode(&self, bytes: &[u8]) -> Result<T, StoreError> {
        let envelope = Envelope::parse(bytes)?;
        self.schema.read(&envelope, &envelope.payload)
    }
}

/// Encrypts the data with `C` and stores it under `ciphertext`.
pub struct Sealed<T, C> {
    schema: Schema<T>,
    cipher: C,
}

impl<T: Serialize + DeserializeOwned, C: Cipher> Sealed<T, C> {
    /// A codec for `app`'s files at `schema`, sealed with `cipher`.
    pub fn new(app: &str, schema: u32, cipher: C) -> Self {
        Sealed { schema: Schema::new(app, schema), cipher }
    }

    /// Reads files from older schemas with `upgrade`, given the opened
    /// payload. Without one, they are read as if current.
    pub fn upgrade(mut self, upgrade: Upgrade<T>) -> Self {
        self.schema.upgrade = Some(upgrade);
        self
    }

    /// Writes `name: value` in cleartext in every file, for older builds of
    /// the app that look for it there.
    pub fn header_field(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.schema.extra.insert(name.into(), value.into());
        self
    }

    pub fn cipher(&self) -> &C {
        &self.cipher
    }

    /// Opens a file already split into an [`Envelope`].
    pub fn open(&self, envelope: &Envelope) -> Result<T, StoreError> {
        envelope.check(&self.schema.app, self.schema.schema)?;
        let plaintext = self.cipher.open(&envelope.payload)?;
        self.schema.read(envelope, &plaintext)
    }
}

impl<T: Serialize + DeserializeOwned, C: Cipher> Codec for Sealed<T, C> {
    type Data = T;

    fn encode(&self, data: &T) -> Result<Vec<u8>, StoreError> {
        let plaintext = Zeroizing::new(serde_json::to_vec(data).map_err(corrupt)?);
        let sealed = self.cipher.seal(&plaintext)?;
        self.schema.file(self.cipher.params(), "ciphertext", B64.encode(sealed).into())
    }

    fn decode(&self, bytes: &[u8]) -> Result<T, StoreError> {
        self.open(&Envelope::parse(bytes)?)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Not encryption: a keyed XOR with a check byte, enough to tell keys
    /// apart and to keep the payload from being readable as JSON.
    pub struct XorCipher(pub u8);

    impl Cipher for XorCipher {
        fn params(&self) -> Map<String, Value> {
            let mut m = Map::new();
            m.insert("salt".into(), "abc".into());
            m
        }
        fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, StoreError> {
            let mut out = vec![self.0];
            out.extend(plaintext.iter().map(|b| b ^ self.0));
            Ok(out)
        }
        fn open(&self, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, KeyMismatch> {
            match sealed.split_first() {
                Some((&check, rest)) if check == self.0 => Ok(Zeroizing::new(rest.iter().map(|b| b ^ self.0).collect())),
                _ => Err(KeyMismatch),
            }
        }
    }

    type Data = BTreeMap<String, u32>;

    fn sample() -> Data {
        [("secret-id".to_string(), 7)].into()
    }

    #[test]
    fn json_round_trips_with_its_header() {
        let codec = Json::<Data>::new("todo", 3);
        let bytes = codec.encode(&sample()).unwrap();
        let file: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(file["magic"], MAGIC);
        assert_eq!(file["app"], "todo");
        assert_eq!(file["schema"], 3);
        assert_eq!(codec.decode(&bytes).unwrap(), sample());
    }

    #[test]
    fn sealed_keeps_everything_but_the_header_and_params_off_disk() {
        let codec = Sealed::<Data, _>::new("vault", 2, XorCipher(9)).header_field("version", 2);
        let bytes = codec.encode(&sample()).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(!text.contains("secret-id"));
        let file: Map<String, Value> = serde_json::from_slice(&bytes).unwrap();
        let mut keys: Vec<&str> = file.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["app", "ciphertext", "format", "magic", "salt", "schema", "version"]);
        assert_eq!(codec.decode(&bytes).unwrap(), sample());

        assert!(matches!(
            Sealed::<Data, _>::new("vault", 2, XorCipher(8)).decode(&bytes),
            Err(StoreError::KeyMismatch)
        ));
        assert_eq!(read_params(&bytes).unwrap()["salt"], "abc");
    }

    #[test]
    fn refuses_newer_schemas_and_other_apps_and_upgrades_older_ones() {
        let old = Json::<Data>::new("todo", 1).encode(&sample()).unwrap();
        let newer = Json::<Data>::new("todo", 5).encode(&sample()).unwrap();
        let other = Json::<Data>::new("notes", 1).encode(&sample()).unwrap();

        let codec = Json::<Data>::new("todo", 2).upgrade(|schema, payload| {
            assert_eq!(schema, 1);
            let mut d: Data = serde_json::from_slice(payload).map_err(|e| e.to_string())?;
            d.insert("upgraded".into(), 1);
            Ok(d)
        });
        assert_eq!(codec.decode(&old).unwrap().get("upgraded"), Some(&1));
        assert!(matches!(codec.decode(&newer), Err(StoreError::Format(FormatError::Newer { .. }))));
        assert!(matches!(codec.decode(&other), Err(StoreError::Format(FormatError::OtherApp(_)))));
        assert!(matches!(codec.decode(b"{\"data\":{}}"), Err(StoreError::Format(FormatError::NotOurs))));
        assert!(matches!(codec.decode(b"not json"), Err(StoreError::Corrupt(_))));
    }
}
