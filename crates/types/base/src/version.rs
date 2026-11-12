//! Module includes various types and logic pertaining to the versioning system under Scroll's
//! proving system.
//!
//! In order to identify the appropriate EVM fork logic and codec version for the encoded data, we
//! make use of a versioning system specified by the tuple `(domain,stf_version)`.
//!
//! The [`Domain`] indicates the execution of "what" are we proving?
//!
//! The [`STFVersion`] indicates an incremental version used within that domain.
//!
//! The tuple `(domain,stf_version)` maps to a unique tuple `(fork,codec)` that tells us which EVM fork
//! logic and codec logic must be utilised to decode data and execute transactions that are being
//! proven.
//!
//! The version is a single byte that encodes the tuple `(domain,stf_version)` such that:
//!
//! | bit index | purpose     |
//! |-----------|-------------|
//! | 0..=1     | domain      |
//! | 2..=7     | stf_version |

use crate::public_inputs::ForkName;

/// Protocol `domain` in the protocol=(domain,version) tuple.
///
/// Domain is represented using 2 bits, i.e. we can support at the most 4 domains as per the latest
/// codec.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Domain {
    /// Domain used for Scroll.
    Scroll = 0,
    /// Domain used for L3 validiums running on L2 Scroll.
    Validium = 1,
}

impl From<u8> for Domain {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Scroll,
            1 => Self::Validium,
            value => unreachable!("unsupported domain={value}"),
        }
    }
}

/// The state-transition-function's incremental version.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub enum STFVersion {
    /// Validium@v1.
    V1 = 1,
    /// Scroll@v6.
    V6 = 6,
    /// Scroll@v7.
    V7 = 7,
    /// Scroll@v8.
    V8 = 8,
    /// Scroll@v9.
    V9 = 9,
    /// Scroll@v10.
    V10 = 10,
    /// DogeOS@v11 (Tsuki). Kept so hosts can name the codec of Tsuki batches; it has no
    /// [`Version`] and no public-input definitions, so nothing is proven under it. Its batches
    /// carry the earlier public-input meaning (state-root endpoints, masked queue hash).
    V11 = 11,
    /// DogeOS@v12 (TsukiV2).
    V12 = 12,
}

impl From<u8> for STFVersion {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::V1,
            6 => Self::V6,
            7 => Self::V7,
            8 => Self::V8,
            9 => Self::V9,
            10 => Self::V10,
            11 => Self::V11,
            12 => Self::V12,
            value => unreachable!("unsupported stf-version={value}"),
        }
    }
}

/// The codec version.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub enum Codec {
    /// da-codec@v6.
    V6,
    /// da-codec@v7.
    ///
    /// Between EuclidV2, Feynman and Galileo hardforks, i.e. STF versions 7, 8 and 9, the da-codec
    /// implementation remains unchanged. As a result, we use the Codec::V7 for each of those
    /// hardforks.
    V7,
}

impl From<Codec> for u8 {
    fn from(value: Codec) -> Self {
        match value {
            Codec::V6 => 6,
            Codec::V7 => 7,
        }
    }
}

/// The number of bits used for [`STFVersion`].
const N_BITS_STF_VERSION: u8 = 6;

/// Mask and get the [`STFVersion`] from the version byte.
const MASK_STF_VERSION: u8 = 0b00111111;

/// A fully parsed version that includes all necessary identifiers.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct Version {
    /// Domain.
    pub domain: Domain,
    /// STF-version.
    pub stf_version: STFVersion,
    /// EVM fork name.
    pub fork: ForkName,
    /// DA-codec version.
    pub codec: Codec,
}

impl Version {
    pub const fn as_version_byte(&self) -> u8 {
        ((self.domain as u8) << N_BITS_STF_VERSION) + (self.stf_version as u8)
    }

    pub const fn euclid_v1() -> Self {
        Self {
            domain: Domain::Scroll,
            stf_version: STFVersion::V6,
            fork: ForkName::EuclidV1,
            codec: Codec::V6,
        }
    }

    pub const fn euclid_v2() -> Self {
        Self {
            domain: Domain::Scroll,
            stf_version: STFVersion::V7,
            fork: ForkName::EuclidV2,
            codec: Codec::V7,
        }
    }

    pub const fn feynman() -> Self {
        Self {
            domain: Domain::Scroll,
            stf_version: STFVersion::V8,
            fork: ForkName::Feynman,
            codec: Codec::V7,
        }
    }

    pub const fn galileo() -> Self {
        Self {
            domain: Domain::Scroll,
            stf_version: STFVersion::V9,
            fork: ForkName::Galileo,
            codec: Codec::V7,
        }
    }

    pub const fn galileo_v2() -> Self {
        Self {
            domain: Domain::Scroll,
            stf_version: STFVersion::V10,
            fork: ForkName::GalileoV2,
            codec: Codec::V7,
        }
    }

    pub const fn validium_v1() -> Self {
        Self {
            domain: Domain::Validium,
            stf_version: STFVersion::V1,
            fork: ForkName::GalileoV2,
            codec: Codec::V7,
        }
    }

    pub fn is_validium(&self) -> bool {
        self.domain == Domain::Validium
    }

    pub const fn tsuki_v2() -> Self {
        Self {
            domain: Domain::Scroll,
            stf_version: STFVersion::V12,
            fork: ForkName::TsukiV2,
            codec: Codec::V7,
        }
    }

    pub fn codec(&self) -> u8 {
        self.codec.into()
    }
}

impl Default for Version {
    fn default() -> Self {
        Self::galileo()
    }
}

impl From<u8> for Version {
    fn from(value: u8) -> Self {
        let domain = Domain::from(value >> N_BITS_STF_VERSION);
        let stf_version = STFVersion::from(value & MASK_STF_VERSION);

        match (domain, stf_version) {
            (Domain::Scroll, STFVersion::V6) => Self::euclid_v1(),
            (Domain::Scroll, STFVersion::V7) => Self::euclid_v2(),
            (Domain::Scroll, STFVersion::V8) => Self::feynman(),
            (Domain::Scroll, STFVersion::V9) => Self::galileo(),
            (Domain::Scroll, STFVersion::V10) => Self::galileo_v2(),
            (Domain::Scroll, STFVersion::V12) => Self::tsuki_v2(),
            (Domain::Validium, STFVersion::V1) => Self::validium_v1(),
            (domain, stf_version) => {
                unreachable!("unsupported version=({domain:?}, {stf_version:?})")
            }
        }
    }
}

/// Version byte for Validium @ v1.
pub const VALIDIUM_V1: u8 = Version::validium_v1().as_version_byte();

/// Version byte for Galileo @ v9.
pub const SCROLL_GALILEO: u8 = Version::galileo().as_version_byte();

/// Version byte for GalileoV2 @ v10.
pub const SCROLL_GALILEO_V2: u8 = Version::galileo_v2().as_version_byte();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsuki_v2_is_version_byte_12() {
        let version = Version::tsuki_v2();
        assert_eq!(version.as_version_byte(), 12);
        let parsed = Version::from(12);
        assert_eq!(parsed.fork, ForkName::TsukiV2);
        assert_eq!(parsed.as_version_byte(), 12);
    }

    /// DogeOS's v11 (Tsuki) public inputs had state-root endpoints and a masked queue hash; the
    /// current definitions are TsukiV2's, so a v11 version byte must not select them.
    #[test]
    #[should_panic(expected = "unsupported version=(Scroll, V11)")]
    fn version_byte_11_is_unsupported() {
        let _ = Version::from(11);
    }

    #[test]
    #[should_panic(expected = "hardfork not accepted: tsuki")]
    fn tsuki_fork_name_is_unsupported() {
        let _ = ForkName::from("tsuki");
    }

    #[test]
    fn tsuki_v2_fork_name_round_trips() {
        assert_eq!(ForkName::from("tsukiv2"), ForkName::TsukiV2);
        assert_eq!(ForkName::TsukiV2.as_str(), "tsukiv2");
    }
}
