//! # Pure Rust SHA-256 Implementation
//!
//! A zero-dependency, self-contained implementation of the SHA-256 secure hash
//! algorithm conforming strictly to FIPS PUB 180-4 and RFC 6234.
//!
//! ## Mathematical Overview:
//! SHA-256 operates on 512-bit message blocks and produces a 256-bit (32-byte) digest.
//! It maintains an internal state of eight 32-bit words (H0..H7) updated through 64 rounds
//! of non-linear logical functions and modular addition.

/// The 64 constant 32-bit words representing the first 32 bits of the fractional parts
/// of the cube roots of the first 64 prime numbers (2 to 311).
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Choice function: selects bits from `y` if `x` bit is set, otherwise from `z`.
/// Formula: (x & y) ^ (~x & z)
#[inline(always)]
fn ch(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (!x & z)
}

/// Majority function: returns the bit value shared by the majority of inputs.
/// Formula: (x & y) ^ (x & z) ^ (y & z)
#[inline(always)]
fn maj(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (x & z) ^ (y & z)
}

/// Upper-case Sigma 0 function for round compression.
/// Formula: ROTR^2(x) ^ ROTR^13(x) ^ ROTR^22(x)
#[inline(always)]
fn sigma0(x: u32) -> u32 {
    x.rotate_right(2) ^ x.rotate_right(13) ^ x.rotate_right(22)
}

/// Upper-case Sigma 1 function for round compression.
/// Formula: ROTR^6(x) ^ ROTR^11(x) ^ ROTR^25(x)
#[inline(always)]
fn sigma1(x: u32) -> u32 {
    x.rotate_right(6) ^ x.rotate_right(11) ^ x.rotate_right(25)
}

/// Lower-case Gamma 0 function for message schedule expansion.
/// Formula: ROTR^7(x) ^ ROTR^18(x) ^ SHR^3(x)
#[inline(always)]
fn gamma0(x: u32) -> u32 {
    x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3)
}

/// Lower-case Gamma 1 function for message schedule expansion.
/// Formula: ROTR^17(x) ^ ROTR^19(x) ^ SHR^10(x)
#[inline(always)]
fn gamma1(x: u32) -> u32 {
    x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10)
}

/// Incremental SHA-256 state machine.
pub struct Sha256 {
    /// Eight 32-bit words representing the current hash state.
    state: [u32; 8],
    /// 64-byte buffer storing incoming message bytes until a full block is ready.
    buffer: [u8; 64],
    /// Number of valid bytes currently in `buffer`.
    buf_len: usize,
    /// Total number of input bytes processed across all updates.
    total_len_bytes: u64,
}

impl Sha256 {
    /// Initializes a new SHA-256 instance with the standard initial hash values.
    /// These are the first 32 bits of the fractional parts of the square roots of primes 2..19.
    pub fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buf_len: 0,
            total_len_bytes: 0,
        }
    }

    /// Feeds additional bytes into the hasher. Processes 64-byte blocks as they become full.
    pub fn update(&mut self, data: &[u8]) {
        self.total_len_bytes += data.len() as u64;
        let mut offset = 0;

        while offset < data.len() {
            let space = 64 - self.buf_len;
            let to_copy = (data.len() - offset).min(space);
            self.buffer[self.buf_len..self.buf_len + to_copy]
                .copy_from_slice(&data[offset..offset + to_copy]);
            self.buf_len += to_copy;
            offset += to_copy;

            if self.buf_len == 64 {
                self.process_block();
                self.buf_len = 0;
            }
        }
    }

    /// Performs the 64 compression rounds on the current 64-byte `buffer`.
    fn process_block(&mut self) {
        // Step 1: Prepare the 64-entry message schedule W
        let mut w = [0u32; 64];
        for i in 0..16 {
            let j = i * 4;
            w[i] = u32::from_be_bytes([
                self.buffer[j],
                self.buffer[j + 1],
                self.buffer[j + 2],
                self.buffer[j + 3],
            ]);
        }

        // Expand remaining 48 words using non-linear gamma functions
        for i in 16..64 {
            let s0 = gamma0(w[i - 15]);
            let s1 = gamma1(w[i - 2]);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        // Step 2: Initialize working variables with current hash state
        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        // Step 3: Run the 64 compression rounds
        for i in 0..64 {
            let t1 = h
                .wrapping_add(sigma1(e))
                .wrapping_add(ch(e, f, g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let t2 = sigma0(a).wrapping_add(maj(a, b, c));

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        // Step 4: Add working variables back into state
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }

    /// Completes the hash calculation by applying FIPS 180-4 padding:
    /// 1. Appends a single '1' bit (0x80 byte).
    /// 2. Pads with zeros until the length modulo 64 is 56 bytes.
    /// 3. Appends the 64-bit big-endian integer representing the original message bit-length.
    pub fn finalize(mut self) -> [u8; 32] {
        let total_bits = self.total_len_bytes.wrapping_mul(8);

        // Append bit '1' (byte 0x80)
        self.buffer[self.buf_len] = 0x80;
        self.buf_len += 1;

        // If not enough room for 8-byte length, pad, process block, and reset buffer
        if self.buf_len > 56 {
            self.buffer[self.buf_len..64].fill(0);
            self.process_block();
            self.buf_len = 0;
        }

        // Pad with zeros up to 56 bytes
        self.buffer[self.buf_len..56].fill(0);

        // Append 64-bit big-endian length
        self.buffer[56..64].copy_from_slice(&total_bits.to_be_bytes());
        self.process_block();

        // Convert state words to output bytes
        let mut out = [0u8; 32];
        for i in 0..8 {
            out[i * 4..(i + 1) * 4].copy_from_slice(&self.state[i].to_be_bytes());
        }
        out
    }
}

/// Convenience function to compute the 64-character lowercase hex SHA-256 digest of arbitrary bytes.
pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    to_hex(&digest)
}

/// Converts a byte slice into a lowercase hexadecimal string.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        write!(&mut hex, "{:02x}", b).unwrap();
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        assert_eq!(
            hash_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_abc() {
        assert_eq!(
            hash_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_sha256_long() {
        assert_eq!(
            hash_bytes(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }
}
