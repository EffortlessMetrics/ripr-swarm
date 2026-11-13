//! Deliberately failing replay control; include only for qualification.
//! The diagnostic must be E0277 naming ObservedByteCapture and Deserialize.

use crate::run::ObservedByteCapture;

fn replay(bytes: &[u8]) -> Result<ObservedByteCapture, serde_json::Error> {
    serde_json::from_slice(bytes)
}

const _: fn(&[u8]) -> Result<ObservedByteCapture, serde_json::Error> = replay;
