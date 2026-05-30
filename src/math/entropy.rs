use crate::error::EntropyError;
use crate::math::fixed_point::{fp_div, fp_log2, fp_mul};

/// Computes the Shannon entropy of the current long/short volume distribution.
/// Returns H(t) in 18-decimal fixed-point representation.
///
/// WORKED NUMERIC EXAMPLE (Balanced Market):
/// Let long_vol = 50, short_vol = 50
///
/// 1. Calculate Totals and Probabilities:
///    - total_vol = 100
///    - p_long = fp_div(50, 100) -> (50 * 1e18) / 100 = 500_000_000_000_000_000 (0.5)
///    - p_short = fp_div(50, 100) -> (50 * 1e18) / 100 = 500_000_000_000_000_000 (0.5)
///
/// 2. Calculate Log2 of Probabilities:
///    - log2_p_long = fp_log2(0.5e18) = -1_000_000_000_000_000_000 (-1.0)
///    - log2_p_short = fp_log2(0.5e18) = -1_000_000_000_000_000_000 (-1.0)
///
/// 3. Multiply Probabilities by their Log2:
///    - term1 = fp_mul(0.5e18, -1.0e18) = -500_000_000_000_000_000 (-0.5)
///    - term2 = fp_mul(0.5e18, -1.0e18) = -500_000_000_000_000_000 (-0.5)
///
/// 4. Sum and Negate:
///    - sum = term1 + term2 = -1_000_000_000_000_000_000 (-1.0)
///    - H(t) = -sum = 1_000_000_000_000_000_000 (1.0)
pub fn compute_entropy(long_vol: u64, short_vol: u64) -> Result<i128, EntropyError> {
    // Degenerate case: if the market is entirely one-sided, entropy is exactly 0.
    // This also neatly avoids division-by-zero or log2(0) errors.
    if long_vol == 0 || short_vol == 0 {
        return Ok(0);
    }

    let total_vol = long_vol
        .checked_add(short_vol)
        .ok_or(EntropyError::MathOverflow)?;

    // We can cast to i128 safely because u64::MAX is ~1.8e19, which fits easily inside i128
    let long_i128 = long_vol as i128;
    let short_i128 = short_vol as i128;
    let total_i128 = total_vol as i128;

    // fp_div automatically scales the numerator by SCALE before dividing
    let p_long = fp_div(long_i128, total_i128)?;
    let p_short = fp_div(short_i128, total_i128)?;

    let log2_p_long = fp_log2(p_long)?;
    let log2_p_short = fp_log2(p_short)?;

    let term_long = fp_mul(p_long, log2_p_long)?;
    let term_short = fp_mul(p_short, log2_p_short)?;

    let sum = term_long
        .checked_add(term_short)
        .ok_or(EntropyError::MathOverflow)?;

    // H(t) = -sum(p_i * log2(p_i))
    sum.checked_neg().ok_or(EntropyError::MathOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::fixed_point::SCALE;

    #[test]
    fn test_compute_entropy_balanced() {
        // 50 long, 50 short -> perfectly balanced -> H(t) = 1.0
        let entropy = compute_entropy(50, 50).unwrap();
        assert_eq!(entropy, SCALE); // 1e18
    }

    #[test]
    fn test_compute_entropy_one_sided() {
        // 100 long, 0 short -> entirely one-sided -> H(t) = 0
        let entropy = compute_entropy(100, 0).unwrap();
        assert_eq!(entropy, 0);

        let entropy_short = compute_entropy(0, 500).unwrap();
        assert_eq!(entropy_short, 0);
    }

    #[test]
    fn test_compute_entropy_imbalanced() {
        // 75 long, 25 short -> imbalanced, entropy should be between 0 and 1.0
        // Expected theoretical value is ~0.811
        let entropy = compute_entropy(75, 25).unwrap();

        assert!(entropy > 0);
        assert!(entropy < SCALE);

        // Let's verify it falls roughly near our expected approximation
        // 0.811 * 1e18 = 811_000_000_000_000_000
        let expected_approx = 811_000_000_000_000_000;
        let diff = (entropy - expected_approx).abs();

        // As long as the linear approximation of log2 keeps it within a 5% error margin
        assert!(diff < 50_000_000_000_000_000);
    }
}
