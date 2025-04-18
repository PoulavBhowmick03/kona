//! Protocol constants for the executor.

use alloy_primitives::{Address, B256, address, b256};

/// The address of the L2 to L1 bridge predeploy.
pub(crate) const L2_TO_L1_BRIDGE: Address = address!("4200000000000000000000000000000000000016");

/// The current version of the output root format.
pub(crate) const OUTPUT_ROOT_VERSION: u8 = 0x00;

/// The version byte for the Holocene extra data.
pub(crate) const HOLOCENE_EXTRA_DATA_VERSION: u8 = 0x00;

/// Empty SHA-256 hash.
pub(crate) const SHA256_EMPTY: B256 =
    b256!("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
