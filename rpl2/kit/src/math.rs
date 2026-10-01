//! The arithmetic the rules need: products and comparisons, exactly, in 256 bits.
//!
//! Every amount the chain can hold is below 2^63 ([`lt_note`]), so a product of four amounts is
//! below 2^252. The rules compare such products; they never divide, and the wallet finds the
//! amounts that satisfy them by searching (`host::max_satisfying`). On the guest's RV32IM target
//! this is `mul`/`mulhu` and adds, with no branch that could panic.
//!
//! [`U256`] arithmetic **wraps**: each caller keeps its products in range, and says why next to
//! the call.

/// A note's value, a reserve, a supply: all below 2^63 (the chain's `MAX_NOTE_VALUE`).
#[inline(always)]
pub fn lt_note(x: u64) -> bool {
    (x >> 63) == 0
}

/// `a · b` as `(high, low)`, exactly, with 32-bit halves (no 128-bit library call).
#[inline(never)]
pub fn wide(a: u64, b: u64) -> (u64, u64) {
    let (a0, a1) = (a & 0xffff_ffff, a >> 32);
    let (b0, b1) = (b & 0xffff_ffff, b >> 32);
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    // Three terms, each below 2^32: the middle column cannot overflow.
    let mid = (p00 >> 32) + (p01 & 0xffff_ffff) + (p10 & 0xffff_ffff);
    let lo = (p00 & 0xffff_ffff) | (mid << 32);
    let hi = p11 + (p01 >> 32) + (p10 >> 32) + (mid >> 32);
    (hi, lo)
}

/// A 256-bit unsigned integer: four 64-bit limbs, least significant first. Wrapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U256(pub [u64; 4]);

impl U256 {
    pub const ZERO: U256 = U256([0; 4]);

    #[inline(always)]
    pub fn from(x: u64) -> U256 {
        U256([x, 0, 0, 0])
    }

    /// `self · m`, modulo 2^256.
    #[inline(never)]
    pub fn mul(self, m: u64) -> U256 {
        let mut out = [0u64; 4];
        let mut carry = 0u64;
        for (o, l) in out.iter_mut().zip(self.0.iter()) {
            let (hi, lo) = wide(*l, m);
            let sum = lo.wrapping_add(carry);
            *o = sum;
            carry = hi.wrapping_add((sum < lo) as u64);
        }
        U256(out)
    }

    /// `self · o`, modulo 2^256.
    #[inline(never)]
    pub fn mul256(self, o: U256) -> U256 {
        let mut acc = U256::ZERO;
        let mut shifted = self;
        for l in o.0.iter() {
            acc = acc.add(shifted.mul(*l));
            shifted = U256([0, shifted.0[0], shifted.0[1], shifted.0[2]]);
        }
        acc
    }

    /// `self + o`, modulo 2^256.
    #[inline(never)]
    pub fn add(self, o: U256) -> U256 {
        let mut out = [0u64; 4];
        let mut carry = 0u64;
        for ((r, a), b) in out.iter_mut().zip(self.0.iter()).zip(o.0.iter()) {
            let s1 = a.wrapping_add(*b);
            let s2 = s1.wrapping_add(carry);
            carry = ((s1 < *a) as u64) | ((s2 < s1) as u64);
            *r = s2;
        }
        U256(out)
    }

    /// `self − o`, modulo 2^256: check [`U256::le`] first where it matters.
    #[inline(never)]
    pub fn sub(self, o: U256) -> U256 {
        let mut out = [0u64; 4];
        let mut borrow = 0u64;
        for ((r, a), b) in out.iter_mut().zip(self.0.iter()).zip(o.0.iter()) {
            let d1 = a.wrapping_sub(*b);
            let d2 = d1.wrapping_sub(borrow);
            borrow = ((*a < *b) as u64) | ((d1 < borrow) as u64);
            *r = d2;
        }
        U256(out)
    }

    /// `self ≤ o`.
    #[inline(never)]
    pub fn le(self, o: U256) -> bool {
        // Most significant limb first; the first limb that differs decides.
        let mut decided = false;
        let mut le = true;
        for (a, b) in self.0.iter().rev().zip(o.0.iter().rev()) {
            let differ = a != b;
            le = if !decided & differ { a < b } else { le };
            decided |= differ;
        }
        le
    }

    /// `self < o`.
    #[inline(always)]
    pub fn lt(self, o: U256) -> bool {
        !o.le(self)
    }

    /// The low 64 bits, and whether the value fits them.
    #[inline(always)]
    pub fn low(self) -> (u64, bool) {
        (self.0[0], (self.0[1] | self.0[2] | self.0[3]) == 0)
    }
}

/// `a · b`, exactly.
#[inline(always)]
pub fn prod(a: u64, b: u64) -> U256 {
    U256::from(a).mul(b)
}

/// `a · b · c`, exactly (below 2^192 for three u64s).
#[inline(always)]
pub fn prod3(a: u64, b: u64, c: u64) -> U256 {
    U256::from(a).mul(b).mul(c)
}

/// `a · b · c · d`, exactly (below 2^256 for four u64s).
#[inline(always)]
pub fn prod4(a: u64, b: u64, c: u64, d: u64) -> U256 {
    U256::from(a).mul(b).mul(c).mul(d)
}

/// `a + b` for two amounts, if it stays below 2^63.
#[inline(always)]
pub fn add_note(a: u64, b: u64) -> Option<u64> {
    let s = a.wrapping_add(b);
    if lt_note(a) & lt_note(b) & lt_note(s) { Some(s) } else { None }
}

/// `a − b` for two amounts, if `b ≤ a`.
#[inline(always)]
pub fn sub_note(a: u64, b: u64) -> Option<u64> {
    if lt_note(a) & (b <= a) { Some(a - b) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big(x: u128) -> U256 {
        U256([x as u64, (x >> 64) as u64, 0, 0])
    }

    #[test]
    fn arithmetic_matches_u128() {
        let xs = [0u64, 1, 2, 3, 999, 1 << 31, (1 << 32) - 1, 1 << 32, u64::MAX >> 1, u64::MAX - 7, u64::MAX];
        for &a in &xs {
            for &b in &xs {
                let p = (a as u128) * (b as u128);
                assert_eq!(wide(a, b), ((p >> 64) as u64, p as u64));
                assert_eq!(prod(a, b), big(p));
                assert_eq!(big(a as u128).add(big(b as u128)), big(a as u128 + b as u128));
                assert_eq!(prod(a, b).le(prod(b, a)), true);
                assert_eq!(big(a as u128).le(big(b as u128)), a <= b);
                assert_eq!(big(a as u128).lt(big(b as u128)), a < b);
                if a >= b {
                    assert_eq!(big(a as u128).sub(big(b as u128)), big((a - b) as u128));
                }
            }
        }
        // Across limbs: (2^64 − 1)^3 and a 256-bit square.
        let m = u64::MAX as u128;
        let c = prod3(u64::MAX, u64::MAX, u64::MAX);
        let sq = m * m;
        assert_eq!(c, big(sq).mul(u64::MAX));
        assert_eq!(big(sq).mul256(big(sq)), c.mul(u64::MAX));
        assert!(prod(u64::MAX, u64::MAX).lt(c));
        assert!(!c.le(prod(u64::MAX, u64::MAX)));
        assert_eq!(c.sub(c), U256::ZERO);
        assert_eq!(U256::ZERO.sub(U256::from(1)).add(U256::from(1)), U256::ZERO);
    }
}
