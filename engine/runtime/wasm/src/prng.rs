//! Deterministic PRNG — seed derived from BLAKE3 of the manifest.
//!
//! The workload never gets access to host entropy. Instead, we derive a
//! deterministic seed from the run manifest so that same manifest = same
//! random sequence.

/// Deterministic PRNG state using xoshiro256** seeded via BLAKE3.
#[derive(Debug, Clone)]
pub struct DeterministicPrng {
    state: [u64; 4],
}

impl DeterministicPrng {
    /// Create a new PRNG seeded from the BLAKE3 hash of the given manifest bytes.
    pub fn from_manifest(manifest_bytes: &[u8]) -> Self {
        let hash = blake3::hash(manifest_bytes);
        let bytes = hash.as_bytes();

        let state = [
            u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        ];

        Self { state }
    }

    /// Create a PRNG from an explicit 32-byte seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let state = [
            u64::from_le_bytes(seed[0..8].try_into().unwrap()),
            u64::from_le_bytes(seed[8..16].try_into().unwrap()),
            u64::from_le_bytes(seed[16..24].try_into().unwrap()),
            u64::from_le_bytes(seed[24..32].try_into().unwrap()),
        ];
        Self { state }
    }

    /// Generate the next u64.
    pub fn next_u64(&mut self) -> u64 {
        let result = (self.state[1].wrapping_mul(5))
            .rotate_left(7)
            .wrapping_mul(9);

        let t = self.state[1] << 17;

        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];

        self.state[2] ^= t;
        self.state[3] = self.state[3].rotate_left(45);

        result
    }

    /// Fill a buffer with deterministic random bytes.
    pub fn fill_bytes(&mut self, buf: &mut [u8]) {
        let mut i = 0;
        while i < buf.len() {
            let val = self.next_u64();
            let bytes = val.to_le_bytes();
            let remaining = buf.len() - i;
            let to_copy = remaining.min(8);
            buf[i..i + to_copy].copy_from_slice(&bytes[..to_copy]);
            i += to_copy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_output() {
        let manifest = b"test-manifest-content";
        let mut prng1 = DeterministicPrng::from_manifest(manifest);
        let mut prng2 = DeterministicPrng::from_manifest(manifest);

        for _ in 0..1000 {
            assert_eq!(prng1.next_u64(), prng2.next_u64());
        }
    }

    #[test]
    fn different_manifest_different_output() {
        let mut prng1 = DeterministicPrng::from_manifest(b"manifest-a");
        let mut prng2 = DeterministicPrng::from_manifest(b"manifest-b");
        // Very unlikely to match
        assert_ne!(prng1.next_u64(), prng2.next_u64());
    }
}
