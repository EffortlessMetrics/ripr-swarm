//! Deliberately failing sibling-module control; include only for qualification.
//! The diagnostic must be E0451 on ObservedByteCapture's private fields.

use crate::run::byte_capture::{ByteCaptureCleanup, RequestedInvocation};
use crate::run::{ByteCaptureBudget, ObservedByteCapture, TimedBytesOutput};
use std::time::Duration;

fn forge(
    invocation: RequestedInvocation,
    output: TimedBytesOutput,
    budget: ByteCaptureBudget,
    cleanup: ByteCaptureCleanup,
    elapsed: Duration,
) -> ObservedByteCapture {
    ObservedByteCapture {
        invocation,
        output,
        budget,
        cleanup,
        elapsed,
    }
}

const _: fn(
    RequestedInvocation,
    TimedBytesOutput,
    ByteCaptureBudget,
    ByteCaptureCleanup,
    Duration,
) -> ObservedByteCapture = forge;
