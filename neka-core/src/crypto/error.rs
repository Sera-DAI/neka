use core::fmt;

/// Crypto errors. Static text: never includes a key, password or data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoError {
    /// KDF params outside the accepted limits.
    ParamsOutOfLimits,
    /// The KDF failed (for example, out of memory).
    KdfFailed,
    /// Tag does not match: wrong key or tampered data.
    AuthenticationFailed,
    /// The operating system did not provide randomness.
    RandomnessUnavailable,
    /// Invalid input size.
    InvalidSize,
    /// Password generator request that cannot be satisfied.
    InvalidGenerator,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ParamsOutOfLimits => "key derivation parameters out of bounds",
            Self::KdfFailed => "key derivation failed",
            Self::AuthenticationFailed => "incorrect password or tampered file",
            Self::RandomnessUnavailable => "randomness source unavailable",
            Self::InvalidSize => "invalid data size",
            Self::InvalidGenerator => "invalid request to the password generator",
        })
    }
}

impl std::error::Error for CryptoError {}
