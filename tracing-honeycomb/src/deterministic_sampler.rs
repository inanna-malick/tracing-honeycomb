use sha1::{Digest, Sha1};

use crate::TraceId;

/// A port of beeline-nodejs's code for the same functionality.
///
/// Samples deterministically on a given TraceId via a SHA-1 hash.
///
/// https://github.com/honeycombio/beeline-nodejs/blob/main/lib/deterministic_sampler.js
pub(crate) fn sample(sample_rate: u32, trace_id: &TraceId) -> bool {
    let sum = Sha1::digest(trace_id.as_ref());
    // Since we are operating on u32's in rust, there is no need for the original's `>>> 0`.
    let upper_bound = std::u32::MAX / sample_rate;

    u32::from_be_bytes([sum[0], sum[1], sum[2], sum[3]]) <= upper_bound
}
