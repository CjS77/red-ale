//! Sine and cosine without floating point, for pie slices (`design/reports.md` §8, "Pie slices"): a fixed-point
//! CORDIC in Q2.61 on `i64` with 62 iterations. The same input gives the same bits on every platform.

use num_traits::ToPrimitive;

use super::geom::div_round;
use crate::number::{Rational, Rounding};

/// The number of fraction bits.
const FRAC_BITS: u32 = 61;

/// π/2 in Q2.61.
const HALF_PI: i64 = 3_622_009_729_038_561_421;

/// The CORDIC gain correction, ∏ 1/√(1 + 2⁻²ⁱ) for i in 0..62, in Q2.61.
const GAIN: i64 = 1_400_229_935_014_726_477;

/// atan(2⁻ⁱ) in Q2.61 for i in 0..62, rounded to nearest.
const ATAN: [i64; 62] = [
    1_811_004_864_519_280_711,
    1_069_098_597_953_152_948,
    564_882_337_777_596_249,
    286_743_094_836_456_889,
    143_927_976_672_616_092,
    72_034_151_524_184_357,
    36_025_865_417_378_411,
    18_014_032_019_027_246,
    9_007_153_442_175_927,
    4_503_593_900_760_542,
    2_251_799_097_857_775,
    1_125_899_817_364_151,
    562_949_942_236_502,
    281_474_975_312_555,
    140_737_488_180_565,
    70_368_744_155_819,
    35_184_372_086_101,
    17_592_186_044_075,
    8_796_093_022_165,
    4_398_046_511_099,
    2_199_023_255_551,
    1_099_511_627_776,
    549_755_813_888,
    274_877_906_944,
    137_438_953_472,
    68_719_476_736,
    34_359_738_368,
    17_179_869_184,
    8_589_934_592,
    4_294_967_296,
    2_147_483_648,
    1_073_741_824,
    536_870_912,
    268_435_456,
    134_217_728,
    67_108_864,
    33_554_432,
    16_777_216,
    8_388_608,
    4_194_304,
    2_097_152,
    1_048_576,
    524_288,
    262_144,
    131_072,
    65_536,
    32_768,
    16_384,
    8_192,
    4_096,
    2_048,
    1_024,
    512,
    256,
    128,
    64,
    32,
    16,
    8,
    4,
    2,
    1,
];

/// sin and cos of an angle φ in [0, π/2], given in Q2.61, both in Q2.61.
fn sin_cos(phi: i64) -> (i64, i64) {
    let (x, y, _) = ATAN.iter().zip(0_u32..).fold((GAIN, 0_i64, phi), |(x, y, z), (atan, i)| {
        if z >= 0 {
            (x - (y >> i), y + (x >> i), z - atan)
        } else {
            (x + (y >> i), y - (x >> i), z + atan)
        }
    });
    (y, x)
}

/// sin and cos of a whole turn's fraction `f` (2πf radians), in Q2.61. The fraction is reduced exactly to a quarter
/// turn and an angle within it, so the CORDIC only ever sees [0, π/2).
pub(super) fn sin_cos_turn(f: &Rational) -> (i64, i64) {
    let in_quarters = f * &Rational::from_int(4);
    let whole = in_quarters.round_to(1, Rounding::Floor).unwrap_or_else(|_| Rational::zero());
    let quarters = whole.numer().to_i64().unwrap_or(0);
    let within = &in_quarters - &whole;
    let phi = (&within * &Rational::from_int(HALF_PI))
        .round()
        .numer()
        .to_i64()
        .unwrap_or(0)
        .clamp(0, HALF_PI);
    let (s, c) = sin_cos(phi);
    match quarters.rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// `r × v` for `v` in Q2.61, rounded half to even.
pub(super) fn scale(r: i64, v: i64) -> i64 {
    let product = div_round(i128::from(r) * i128::from(v), 1_i128 << FRAC_BITS);
    i64::try_from(product).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn turn(n: i64, d: i64) -> (i64, i64) {
        let (s, c) = sin_cos_turn(&Rational::new(n, d).unwrap());
        (scale(1_000_000, s), scale(1_000_000, c))
    }

    #[test]
    fn quarter_turns_are_exact_at_a_million() {
        assert_eq!(turn(0, 1), (0, 1_000_000));
        assert_eq!(turn(1, 4), (1_000_000, 0));
        assert_eq!(turn(1, 2), (0, -1_000_000));
        assert_eq!(turn(3, 4), (-1_000_000, 0));
        assert_eq!(turn(1, 1), (0, 1_000_000));
        assert_eq!(turn(1, 8), (707_107, 707_107));
        assert_eq!(turn(1, 12), (500_000, 866_025));
        assert_eq!(turn(7, 12), (-500_000, -866_025));
    }

    proptest! {
        /// sin² + cos² = 1 to within the rounding of a coordinate at a radius of a million.
        #[test]
        fn points_stay_on_the_circle(n in 0_i64..1_000_000, d in 1_i64..1_000_000) {
            let (s, c) = turn(n % (d + 1), d);
            let r2 = i128::from(s) * i128::from(s) + i128::from(c) * i128::from(c);
            prop_assert!((r2 - 1_000_000_000_000).abs() <= 2_000_001, "{s} {c}");
        }
    }
}
