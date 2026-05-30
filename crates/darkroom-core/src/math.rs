//! Fast math approximations shared by multiple IOPs.
//!
//! Mirrors the inline helpers in src/common/math.h. These are bit-twiddled
//! polynomial approximations to logf / expf with documented error bounds
//! and matching constants, so the Rust pipeline produces byte-identical
//! pixel output to the C path even when the host CPU lacks SVML.

pub const M_LN2: f32 = std::f32::consts::LN_2;

/// IEEE-754 polynomial approximation of log2(x), matching `fastlog2()` in
/// src/common/math.h byte-for-byte.
///
/// Valid for positive x; behaviour for x <= 0 is undefined (the C version
/// reads the float bits unconditionally and returns garbage rather than NaN).
#[inline(always)]
pub fn fastlog2(x: f32) -> f32 {
    let vx_i = x.to_bits();
    let mx_i = (vx_i & 0x007F_FFFF) | 0x3f00_0000;
    let mx_f = f32::from_bits(mx_i);
    let y = vx_i as f32 * 1.1920928955078125e-7_f32;

    y - 124.22551499
        - 1.498030302 * mx_f
        - 1.72587999 / (0.3520887068 + mx_f)
}

/// Natural log via `fastlog2`, matching `fastlog()` in src/common/math.h.
#[inline(always)]
pub fn fastlog(x: f32) -> f32 {
    M_LN2 * fastlog2(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fastlog2_approximates_log2() {
        for &x in &[0.25_f32, 0.5, 1.0, 2.0, 4.0, 10.0, 100.0] {
            let approx = fastlog2(x);
            let exact = x.log2();
            assert!((approx - exact).abs() < 0.05, "x={x} approx={approx} exact={exact}");
        }
    }

    #[test]
    fn fastlog_one_is_near_zero() {
        let r = fastlog(1.0);
        assert!(r.abs() < 0.05);
    }

    #[test]
    fn fastlog_uses_natural_base() {
        // fastlog(e) ≈ 1
        let r = fastlog(std::f32::consts::E);
        assert!((r - 1.0).abs() < 0.05);
    }
}
