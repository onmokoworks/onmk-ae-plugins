//! Stateless per-line random numbers. Every value is a pure function of
//! (seed, line id, channel, epoch), so any frame can be rendered in any order.

pub const CH_ANGLE_OFFSET: u32 = 1;
pub const CH_JITTER: u32 = 2;
pub const CH_WIDTH: u32 = 3;
pub const CH_LENGTH: u32 = 4;
pub const CH_SPREAD: u32 = 5;
pub const CH_STAGGER: u32 = 6;
pub const CH_CLUSTER_OFFSET: u32 = 7;
pub const CH_DENSITY: u32 = 8;
pub const CH_SHORT_GROUP: u32 = 9;
pub const CH_SHORT_LENGTH: u32 = 10;

/// splitmix64 finalizer.
pub fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Uniform value in [0, 1).
pub fn unit(seed: u32, line: u32, channel: u32, epoch: i64) -> f64 {
    let mut h = mix64(0x464F_4355_534C_494E ^ u64::from(seed));
    h = mix64(h ^ u64::from(line));
    h = mix64(h ^ ((u64::from(channel) << 1) | 1));
    h = mix64(h ^ epoch as u64);
    (h >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_is_pure_and_in_range() {
        for k in 0..2000 {
            let a = unit(7, k, CH_WIDTH, -3);
            assert_eq!(a.to_bits(), unit(7, k, CH_WIDTH, -3).to_bits());
            assert!((0.0..1.0).contains(&a));
        }
        assert_ne!(unit(1, 5, CH_WIDTH, 0), unit(2, 5, CH_WIDTH, 0));
        assert_ne!(unit(1, 5, CH_WIDTH, 0), unit(1, 5, CH_LENGTH, 0));
        assert_ne!(unit(1, 5, CH_WIDTH, 0), unit(1, 5, CH_WIDTH, 1));
    }
}
