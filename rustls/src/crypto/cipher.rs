use crate::enums::{ContentType, ProtocolVersion};
use crate::error::Error;
use crate::msgs::codec;
pub use crate::msgs::message::{BorrowedPlainMessage, OpaqueMessage, PlainMessage};

/// Objects with this trait can decrypt TLS messages.
pub trait MessageDecrypter: Send + Sync {
    /// Perform the decryption over the concerned TLS message.
    fn decrypt(&self, m: OpaqueMessage, seq: u64) -> Result<PlainMessage, Error>;
}

/// Objects with this trait can encrypt TLS messages.
pub(crate) trait MessageEncrypter: Send + Sync {
    fn encrypt(&self, m: BorrowedPlainMessage, seq: u64) -> Result<OpaqueMessage, Error>;
}

impl dyn MessageEncrypter {
    pub(crate) fn invalid() -> Box<dyn MessageEncrypter> {
        Box::new(InvalidMessageEncrypter {})
    }
}

impl dyn MessageDecrypter {
    pub(crate) fn invalid() -> Box<dyn MessageDecrypter> {
        Box::new(InvalidMessageDecrypter {})
    }
}

/// A write or read IV.
#[derive(Default)]
pub struct Iv(pub(crate) [u8; NONCE_LEN]);

impl Iv {
    #[cfg(feature = "tls12")]
    fn new(value: [u8; NONCE_LEN]) -> Self {
        Self(value)
    }

    #[cfg(feature = "tls12")]
    pub(crate) fn copy(value: &[u8]) -> Self {
        debug_assert_eq!(value.len(), NONCE_LEN);
        let mut iv = Self::new(Default::default());
        iv.0.copy_from_slice(value);
        iv
    }

    #[cfg(test)]
    pub(crate) fn value(&self) -> &[u8; NONCE_LEN] {
        &self.0
    }
}

impl From<[u8; NONCE_LEN]> for Iv {
    fn from(bytes: [u8; NONCE_LEN]) -> Self {
        Self(bytes)
    }
}

/// A nonce.  This is unique for all messages on a connection.
pub struct Nonce(pub [u8; NONCE_LEN]);

impl Nonce {
    /// Combine an `Iv` and sequence number to produce a unique nonce.
    ///
    /// This is `iv ^ seq` where `seq` is encoded as a 96-bit big-endian integer.
    #[inline]
    pub fn new(iv: &Iv, seq: u64) -> Self {
        let mut nonce = Self([0u8; NONCE_LEN]);
        codec::put_u64(seq, &mut nonce.0[4..]);

        nonce
            .0
            .iter_mut()
            .zip(iv.0.iter())
            .for_each(|(nonce, iv)| {
                *nonce ^= *iv;
            });

        nonce
    }
}

/// Size of TLS nonces (incorrectly termed "IV" in standard) for all supported ciphersuites
/// (AES-GCM, Chacha20Poly1305)
const NONCE_LEN: usize = 12;

/// Returns a TLS1.3 `additional_data` encoding.
///
/// See RFC8446 s5.2 for the `additional_data` definition.
#[inline]
pub fn make_tls13_aad(payload_len: usize) -> [u8; 5] {
    [
        ContentType::ApplicationData.get_u8(),
        // nb. this is `legacy_record_version`, ie TLS1.2 even for TLS1.3.
        (ProtocolVersion::TLSv1_2.get_u16() >> 8) as u8,
        (ProtocolVersion::TLSv1_2.get_u16() & 0xff) as u8,
        (payload_len >> 8) as u8,
        (payload_len & 0xff) as u8,
    ]
}

/// Returns a TLS1.2 `additional_data` encoding.
///
/// See RFC5246 s6.2.3.3 for the `additional_data` definition.
#[inline]
pub fn make_tls12_aad(
    seq: u64,
    typ: ContentType,
    vers: ProtocolVersion,
    len: usize,
) -> [u8; TLS12_AAD_SIZE] {
    let mut out = [0; TLS12_AAD_SIZE];
    codec::put_u64(seq, &mut out[0..]);
    out[8] = typ.get_u8();
    codec::put_u16(vers.get_u16(), &mut out[9..]);
    codec::put_u16(len as u16, &mut out[11..]);
    out
}

const TLS12_AAD_SIZE: usize = 8 + 1 + 2 + 2;

/// A key for an AEAD algorithm.
///
/// This is a value type for a byte string up to `AeadKey::MAX_LEN` bytes in length.
pub struct AeadKey {
    buf: [u8; Self::MAX_LEN],
    used: usize,
}

impl AeadKey {
    pub(crate) fn with_length(self, len: usize) -> Self {
        assert!(len <= self.used);
        Self {
            buf: self.buf,
            used: len,
        }
    }

    /// Largest possible AEAD key in the ciphersuites we support.
    pub(crate) const MAX_LEN: usize = 32;
}

impl AsRef<[u8]> for AeadKey {
    fn as_ref(&self) -> &[u8] {
        &self.buf[..self.used]
    }
}

impl From<[u8; Self::MAX_LEN]> for AeadKey {
    fn from(bytes: [u8; Self::MAX_LEN]) -> Self {
        Self {
            buf: bytes,
            used: Self::MAX_LEN,
        }
    }
}

/// A `MessageEncrypter` which doesn't work.
struct InvalidMessageEncrypter {}

impl MessageEncrypter for InvalidMessageEncrypter {
    fn encrypt(&self, _m: BorrowedPlainMessage, _seq: u64) -> Result<OpaqueMessage, Error> {
        Err(Error::EncryptError)
    }
}

/// A `MessageDecrypter` which doesn't work.
struct InvalidMessageDecrypter {}

impl MessageDecrypter for InvalidMessageDecrypter {
    fn decrypt(&self, _m: OpaqueMessage, _seq: u64) -> Result<PlainMessage, Error> {
        Err(Error::DecryptError)
    }
}