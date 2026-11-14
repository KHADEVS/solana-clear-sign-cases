//! Keys, hashes and number formatting shared by the renderer and the generator.

use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use sha2::{Digest, Sha256};

/// A Solana address.
pub type Key = [u8; 32];

pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// A deterministic on-curve public key for a label, so the cases rebuild the
/// same bytes every time. Nobody holds the matching secret key.
pub fn test_key(label: &str) -> Key {
    let scalar = Scalar::from_bytes_mod_order(sha256(&[b"clear-sign-cases:", label.as_bytes()]));
    EdwardsPoint::mul_base(&scalar).compress().to_bytes()
}

/// Solana's program-derived address: the first bump, from 255 down, whose
/// hash is not a point on the curve.
pub fn find_pda(seeds: &[&[u8]], program: &Key) -> (Key, u8) {
    for bump in (0..=255u8).rev() {
        let bump_seed = [bump];
        let mut parts: Vec<&[u8]> = seeds.to_vec();
        parts.push(&bump_seed);
        parts.push(program);
        parts.push(b"ProgramDerivedAddress");
        let hash = sha256(&parts);
        if CompressedEdwardsY(hash).decompress().is_none() {
            return (hash, bump);
        }
    }
    panic!("no program-derived address for these seeds")
}

/// Anchor's 8-byte discriminator, e.g. ("global", "proposal_approve").
pub fn anchor_discriminator(namespace: &str, name: &str) -> [u8; 8] {
    let hash = sha256(&[format!("{namespace}:{name}").as_bytes()]);
    hash[..8].try_into().expect("8 bytes")
}

pub fn b58(key: &Key) -> String {
    bs58::encode(key).into_string()
}

pub fn from_b58(text: &str) -> Result<Key, String> {
    let bytes = bs58::decode(text)
        .into_vec()
        .map_err(|e| format!("bad base58 {text}: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| format!("not a 32-byte address: {text}"))
}

/// sRFC 39's base58 short form: "86xCnPeV69…Mpo2MMY" -> "86xCn...o2MMY".
pub fn short(key: &Key) -> String {
    let full = b58(key);
    if full.len() <= 13 {
        full
    } else {
        format!("{}...{}", &full[..5], &full[full.len() - 5..])
    }
}

/// `value / 10^decimals`, without trailing zeros: 10_000_000_000 with 9
/// decimals is "10", 1_500_000 with 6 is "1.5".
pub fn format_amount(value: u128, decimals: u32) -> String {
    if decimals == 0 {
        return value.to_string();
    }
    let scale = 10u128.pow(decimals);
    let whole = value / scale;
    let fraction = value % scale;
    if fraction == 0 {
        return whole.to_string();
    }
    let digits = format!("{fraction:0width$}", width = decimals as usize);
    format!("{whole}.{}", digits.trim_end_matches('0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_short_form_matches_the_spec_examples() {
        let token_account = from_b58("86xCnPeV69n6t3DnyGvkKobf9FdN2H9oiVDdaMpo2MMY").unwrap();
        assert_eq!(short(&token_account), "86xCn...o2MMY");
        let wsol = from_b58("So11111111111111111111111111111111111111112").unwrap();
        assert_eq!(short(&wsol), "So111...11112");
    }

    #[test]
    fn amounts_drop_trailing_zeros() {
        assert_eq!(format_amount(10_000_000_000, 9), "10");
        assert_eq!(format_amount(1_500_000, 6), "1.5");
        assert_eq!(format_amount(u64::MAX as u128, 6), "18446744073709.551615");
        assert_eq!(format_amount(7, 0), "7");
    }

    #[test]
    fn a_pda_is_off_the_curve() {
        let program = test_key("program");
        let (pda, _) = find_pda(&[b"seed"], &program);
        assert!(CompressedEdwardsY(pda).decompress().is_none());
        assert!(CompressedEdwardsY(test_key("x")).decompress().is_some());
    }
}
