use separator::{separated_float, separated_int, separated_uint_with_output, Separatable};
use spora_consensus_core::constants::*;
use spora_consensus_core::network::NetworkType;

#[inline]
pub fn sau_to_spora(sau: u64) -> f64 {
    sau as f64 / SAU_PER_TONDI as f64
}

#[inline]
pub fn spora_to_sau(spora: f64) -> u64 {
    (spora * SAU_PER_TONDI as f64) as u64
}

#[inline]
pub fn sau_to_spora_string(sau: u64) -> String {
    sau_to_spora(sau).separated_string()
}

#[inline]
pub fn sau_to_spora_string_with_trailing_zeroes(sau: u64) -> String {
    separated_float!(format!("{:.8}", sau_to_spora(sau)))
}

pub fn spora_suffix(network_type: &NetworkType) -> &'static str {
    match network_type {
        NetworkType::Mainnet => "SPORA",
        NetworkType::Testnet => "TTONDI",
        NetworkType::Simnet => "STONDI",
        NetworkType::Devnet => "DTONDI",
    }
}

/// Convert sau to Spora string with suffix
#[inline]
pub fn sau_to_spora_string_with_suffix(sau: u64, network_type: &NetworkType) -> String {
    let spora = sau_to_spora_string(sau);
    let suffix = spora_suffix(network_type);
    format!("{spora} {suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sau_to_spora() {
        // Test basic conversion
        assert_eq!(sau_to_spora(100_000_000), 1.0);
        assert_eq!(sau_to_spora(50_000_000), 0.5);
        assert_eq!(sau_to_spora(200_000_000), 2.0);
        assert_eq!(sau_to_spora(0), 0.0);

        // Test with larger values
        assert_eq!(sau_to_spora(100_000_000_000), 1000.0);
        assert_eq!(sau_to_spora(12_345_678), 0.12345678);
    }

    #[test]
    fn test_spora_to_sau() {
        // Test basic conversion
        assert_eq!(spora_to_sau(1.0), 100_000_000);
        assert_eq!(spora_to_sau(0.5), 50_000_000);
        assert_eq!(spora_to_sau(2.0), 200_000_000);
        assert_eq!(spora_to_sau(0.0), 0);

        // Test with larger values
        assert_eq!(spora_to_sau(1000.0), 100_000_000_000);
        assert_eq!(spora_to_sau(0.12345678), 12_345_678);

        // Test precision handling
        assert_eq!(spora_to_sau(0.00000001), 1);
        assert_eq!(spora_to_sau(0.000000001), 0); // Should round down
    }

    #[test]
    fn test_sau_to_spora_string() {
        // Test basic string conversion
        assert_eq!(sau_to_spora_string(100_000_000), "1");
        assert_eq!(sau_to_spora_string(50_000_000), "0.5");
        assert_eq!(sau_to_spora_string(0), "0");

        // Test with larger values
        assert_eq!(sau_to_spora_string(100_000_000_000), "1,000");
        assert_eq!(sau_to_spora_string(12_345_678), "0.12345678");
    }

    #[test]
    fn test_sau_to_spora_string_with_trailing_zeroes() {
        // Test string conversion with trailing zeroes
        assert_eq!(sau_to_spora_string_with_trailing_zeroes(100_000_000), "1.00000000");
        assert_eq!(sau_to_spora_string_with_trailing_zeroes(50_000_000), "0.50000000");
        assert_eq!(sau_to_spora_string_with_trailing_zeroes(0), "0.00000000");

        // Test with larger values
        assert_eq!(sau_to_spora_string_with_trailing_zeroes(100_000_000_000), "1,000.00000000");
        assert_eq!(sau_to_spora_string_with_trailing_zeroes(12_345_678), "0.12345678");
    }

    #[test]
    fn test_spora_suffix() {
        // Test suffix for different network types
        assert_eq!(spora_suffix(&NetworkType::Mainnet), "SPORA");
        assert_eq!(spora_suffix(&NetworkType::Testnet), "TTONDI");
        assert_eq!(spora_suffix(&NetworkType::Simnet), "STONDI");
        assert_eq!(spora_suffix(&NetworkType::Devnet), "DTONDI");
    }

    #[test]
    fn test_sau_to_spora_string_with_suffix() {
        // Test conversion with suffix for different network types
        assert_eq!(sau_to_spora_string_with_suffix(100_000_000, &NetworkType::Mainnet), "1 SPORA");
        assert_eq!(sau_to_spora_string_with_suffix(50_000_000, &NetworkType::Testnet), "0.5 TTONDI");
        assert_eq!(sau_to_spora_string_with_suffix(200_000_000, &NetworkType::Simnet), "2 STONDI");
        assert_eq!(sau_to_spora_string_with_suffix(0, &NetworkType::Devnet), "0 DTONDI");

        // Test with larger values
        assert_eq!(sau_to_spora_string_with_suffix(100_000_000_000, &NetworkType::Mainnet), "1,000 SPORA");
        assert_eq!(sau_to_spora_string_with_suffix(12_345_678, &NetworkType::Testnet), "0.12345678 TTONDI");
    }

    #[test]
    fn test_conversion_roundtrip() {
        // Test that converting from sau to spora and back gives approximately the same result
        // Note: Due to floating point precision limitations, exact equality may not be possible
        let original_sau = 123_456_789;
        let spora = sau_to_spora(original_sau);
        let converted_sau = spora_to_sau(spora);

        // Allow for small floating point precision errors (within 1 SAU)
        let diff = if converted_sau > original_sau { converted_sau - original_sau } else { original_sau - converted_sau };
        assert!(diff <= 1, "Conversion roundtrip error too large: {} -> {} (diff: {})", original_sau, converted_sau, diff);

        // Test with fractional values - use approximate equality for floating point
        let original_spora = 0.12345678;
        let sau = spora_to_sau(original_spora);
        let converted_spora = sau_to_spora(sau);

        // Allow for small floating point precision errors
        let diff = (original_spora - converted_spora).abs();
        assert!(
            diff < 1e-8,
            "Fractional conversion roundtrip error too large: {} -> {} (diff: {})",
            original_spora,
            converted_spora,
            diff
        );
    }

    #[test]
    fn test_edge_cases() {
        // Test edge cases
        assert_eq!(sau_to_spora(u64::MAX), u64::MAX as f64 / SAU_PER_TONDI as f64);
        assert_eq!(spora_to_sau(f64::MAX), u64::MAX); // Should saturate
        assert_eq!(spora_to_sau(f64::MIN), 0); // Should handle negative values
        assert_eq!(spora_to_sau(f64::NAN), 0); // Should handle NaN
        assert_eq!(spora_to_sau(f64::INFINITY), u64::MAX); // Should handle infinity
    }
}
