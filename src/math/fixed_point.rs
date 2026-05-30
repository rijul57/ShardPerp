use crate::error::EntropyError;

/// 18 decimal fixed-point scale (1.0 = 1e18)
pub const SCALE: i128 = 1_000_000_000_000_000_000;

/// Multiplies two fixed-point numbers: (a * b) / SCALE
pub fn fp_mul(a: i128, b: i128) -> Result<i128, EntropyError> {
    a.checked_mul(b)
        .and_then(|v| v.checked_div(SCALE))
        .ok_or(EntropyError::MathOverflow)
}

/// Divides two fixed-point numbers: (a * SCALE) / b
pub fn fp_div(a: i128, b: i128) -> Result<i128, EntropyError> {
    a.checked_mul(SCALE)
        .and_then(|v| v.checked_div(b))
        .ok_or(EntropyError::MathOverflow)
}

/// Computes the base-2 logarithm of a fixed-point number using linear interpolation.
///
/// WORKED NUMERIC EXAMPLE:
/// Let's calculate fp_log2(0.5 * 1e18) -> expected result is -1.0 * 1e18.
/// x = 500_000_000_000_000_000
///
/// 1. Calculate approx_log2(x):
///    - msb = 127 - x.leading_zeros() = 58
///    - power_of_two = 2^58 = 288,230,376,151,711,744
///    - frac = x - power_of_two = 211,769,623,848,288,256
///    - frac_fixed = (frac * 1e18) / power_of_two ≈ 734,723,812,853,215,264 (0.7347...)
///    - approx_log2(x) = (58 * 1e18) + frac_fixed = 58,734,723,812,853,215,264
///
/// 2. Calculate approx_log2(SCALE):
///    - msb = 127 - SCALE.leading_zeros() = 59
///    - power_of_two = 2^59 = 576,460,752,303,423,488
///    - frac = SCALE - power_of_two = 423,539,247,696,576,512
///    - frac_fixed = (frac * 1e18) / power_of_two ≈ 734,723,812,853,215,264 (0.7347...)
///    - approx_log2(SCALE) = (59 * 1e18) + frac_fixed = 59,734,723,812,853,215,264
///
/// 3. Final calculation:
///    - approx_log2(x) - approx_log2(SCALE)
///    - 58.7347...e18 - 59.7347...e18 = -1,000_000_000_000_000_000 (-1.0)
pub fn fp_log2(x: i128) -> Result<i128, EntropyError> {
    if x <= 0 {
        return Err(EntropyError::MathOverflow); // Log2 is undefined for x <= 0
    }

    let log2_x = approx_log2(x)?;
    let log2_scale = approx_log2(SCALE)?;

    log2_x
        .checked_sub(log2_scale)
        .ok_or(EntropyError::MathOverflow)
}

/// Helper function to get the linearly interpolated log2 of a raw i128 integer
/// shifted into our fixed-point format.
fn approx_log2(n: i128) -> Result<i128, EntropyError> {
    // We use 127 since n is an i128 (128 bits total).
    let msb = 127 - n.leading_zeros();

    let power_of_two = 1_i128.checked_shl(msb).ok_or(EntropyError::MathOverflow)?;

    let frac = n
        .checked_sub(power_of_two)
        .ok_or(EntropyError::MathOverflow)?;

    let msb_fixed = (msb as i128)
        .checked_mul(SCALE)
        .ok_or(EntropyError::MathOverflow)?;

    let frac_fixed = frac
        .checked_mul(SCALE)
        .and_then(|v| v.checked_div(power_of_two))
        .ok_or(EntropyError::MathOverflow)?;

    msb_fixed
        .checked_add(frac_fixed)
        .ok_or(EntropyError::MathOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fp_mul() {
        // 1.5 * 2.0 = 3.0
        let a = 1_500_000_000_000_000_000;
        let b = 2_000_000_000_000_000_000;
        let result = fp_mul(a, b).unwrap();
        assert_eq!(result, 3_000_000_000_000_000_000);
    }

    #[test]
    fn test_fp_div() {
        // 3.0 / 2.0 = 1.5
        let a = 3_000_000_000_000_000_000;
        let b = 2_000_000_000_000_000_000;
        let result = fp_div(a, b).unwrap();
        assert_eq!(result, 1_500_000_000_000_000_000);
    }

    #[test]
    fn test_fp_log2_one() {
        // log2(1.0) = 0
        let result = fp_log2(SCALE).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_fp_log2_half() {
        // log2(0.5) = -1.0
        let half = SCALE / 2;
        let result = fp_log2(half).unwrap();
        assert_eq!(result, -SCALE);
    }
}
