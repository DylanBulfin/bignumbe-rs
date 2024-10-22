use std::{
    cmp::Ordering,
    iter::{Product, Sum},
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

use utils::get_exp_u64;

mod utils;

// Equal to 2^63
const MIN_BASE_VAL: u64 = 0x8000_0000_0000_0000;

/// Marker trait used for types that can be converted into BigNum
/// This is used to allow for easy definition of methods like Add<T>
pub trait BigNumConvertable: Into<BigNum> {}

/// Representation of large number. Formula is base * (2 ^ exp)
#[derive(Debug, Clone, Copy, Eq)]
pub struct BigNum {
    base: u64,
    exp: u64,
    // This field keeps me from accidentally constructing this struct manually
    invalidate: bool,
}

impl BigNum {
    pub const ZERO: BigNum = BigNum {
        base: 0,
        exp: 0,
        invalidate: true,
    };

    pub const ONE: BigNum = BigNum {
        base: 1,
        exp: 0,
        invalidate: true,
    };

    /// Create a BigNum instance directly (e.g. not through the From trait)
    pub fn new(base: u64, exp: u64) -> Self {
        if base == 0 && exp != 0 {
            panic!("Invalid BigNum: base is 0 but exp is {}", exp)
        }
        if base < MIN_BASE_VAL && exp != 0 {
            panic!("Invalid BigNum: exp is {} but base is {:#x}", exp, base)
        }
        BigNum {
            base,
            exp,
            invalidate: false,
        }
    }

    /// The exponent x such that self = c * 2^x for some c between 0 and 1
    pub fn get_full_exp(&self) -> u64 {
        if self.exp == 0 {
            utils::get_exp_u64(self.base)
        } else {
            // Panics when self.exp + 63 > u64::MAX
            self.exp + 63
        }
    }
}

impl PartialEq for BigNum {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base && self.exp == other.exp
    }
}

impl Ord for BigNum {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.exp.cmp(&other.exp) {
            Ordering::Equal => (),
            ord => return ord,
        }
        self.base.cmp(&other.base)
    }
}

impl PartialOrd for BigNum {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Add for BigNum {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let (max, min) = if self > rhs { (self, rhs) } else { (rhs, self) };
        let shift = max.exp - min.exp;

        if shift >= 64 {
            // minimum number is too small to make a difference in the sum
            return max;
        }

        let result: u128 = (max.base as u128) + ((min.base >> shift) as u128);

        if utils::get_exp_u128(result) >= 64 {
            if max.exp == u64::MAX {
                panic!("Attempt to add BigNum with overflow");
            }
            BigNum::new((result >> 1) as u64, 1 + max.exp)
        } else {
            //(result as u64).into()
            BigNum::new((result) as u64, max.exp)
        }
    }
}

pub fn add_old(lhs: BigNum, rhs: BigNum) -> BigNum {
    if lhs.exp == 0 && rhs.exp == 0 {
        // Both numbers are in compact form, first try normal addition
        let result = lhs.base.wrapping_add(rhs.base);

        // If remainder is less than either base, overflow occurred
        if result < lhs.base || result < rhs.base {
            BigNum::new(MIN_BASE_VAL + (result >> 1), 1)
        } else {
            BigNum::new(result, 0)
        }
    } else {
        // At least one of the numbers is in expanded form, first find which is bigger
        let (min, max) = if lhs > rhs { (rhs, lhs) } else { (lhs, rhs) };

        // Calculate how much we need to shift the smaller number to align
        let shift = if min.exp == 0 {
            max.exp
        } else {
            max.get_full_exp() - min.get_full_exp()
        };

        if shift >= 64 || shift > min.get_full_exp() {
            // Shifting will leave us with 0 so don't bother, return max
            max
        } else {
            // Now we can add them, and handle any overflow
            let res = max.base.wrapping_add(min.base >> shift);

            // If result is less than either base, overflow occurred
            if res < max.base {
                // Wrapping occurred, need to fix things up
                BigNum::new(MIN_BASE_VAL + (res >> 1), max.exp + 1)
            } else {
                BigNum::new(res, max.exp)
            }
        }
    }
}

pub fn add_new(lhs: BigNum, rhs: BigNum) -> BigNum {
    let (max, min) = if lhs > rhs { (lhs, rhs) } else { (rhs, lhs) };
    let shift = max.exp - min.exp;

    if shift >= 64 {
        // minimum number is too small to make a difference in the sum
        return max;
    }

    let result: u128 = (max.base as u128) + ((min.base >> shift) as u128);

    if utils::get_exp_u128(result) >= 64 {
        if max.exp == u64::MAX {
            panic!("Attempt to add BigNum with overflow");
        }
        BigNum::new((result >> 1) as u64, 1 + max.exp)
    } else {
        //(result as u64).into()
        BigNum::new((result) as u64, max.exp)
    }
}

pub fn add_newer(lhs: BigNum, rhs: BigNum) -> BigNum {
    let (max, min) = if lhs > rhs { (lhs, rhs) } else { (rhs, lhs) };
    let shift = max.exp - min.exp;

    if shift >= 64 {
        // minimum number is too small to make a difference in the sum
        return max;
    }

    let res = max.base.wrapping_add(min.base >> shift);

    if res < max.base {
        // Wrap occurred, need to normalize value and exp
        BigNum::new((res >> 1) + MIN_BASE_VAL, max.exp + 1)
    } else {
        // No wrap, easy
        BigNum::new(res, max.exp)
    }
}

impl Sub for BigNum {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        if rhs > self {
            // We can't have negative numbers
            panic!("Attempt to subtract with overflow")
        }

        if rhs.exp == 0 && self.exp == 0 {
            // Both are in compact form, and since self > rhs we know we won't underflow
            Self::new(self.base - rhs.base, 0)
        } else {
            // Find how much we need to shift the smaller number to align
            let shift = if rhs.exp == 0 {
                self.exp
            } else {
                self.get_full_exp() - rhs.get_full_exp()
            };

            if shift >= 64 || shift > rhs.get_full_exp() {
                if self.base == MIN_BASE_VAL && (shift == 64 || shift == rhs.get_full_exp() + 1) {
                    // Base is at the minimum value so we need to handle edge case
                    // E.g. BigNum::new(0x8000_0000_0000_0000, 1) - BigNum::from(1)
                    // shift = 1, get_full_exp = 0, so normally we would skip
                    // But since base is at min value we need to decrease exp and normalize
                    BigNum::new(u64::MAX, self.exp - 1)
                } else {
                    // Shifting will leave us with 0 so don't bother, return self
                    self
                }
            } else {
                let res = self.base - (rhs.base >> shift);

                // We know that underflow won't happen, but if numbers are equal
                // we need to handle 0 case
                if res == 0 {
                    Self::new(0, 0)
                } else {
                    // If new resulting base is not in range we need to fix
                    let adjustment = 63 - get_exp_u64(res);

                    Self::new(res << adjustment, self.exp - adjustment)
                }
            }
        }
    }
}

impl Mul for BigNum {
    type Output = Self;

    fn mul(self, rhs: BigNum) -> Self::Output {
        let result: u128 = self.base as u128 * rhs.base as u128;
        let max_pow = utils::get_exp_u128(result) as u64;

        if max_pow < 64 {
            // Result is compact
            BigNum::new(result as u64, self.exp + rhs.exp)
        } else {
            // Result is expanded
            let adj = max_pow - 63;

            BigNum::new((result >> adj) as u64, self.exp + rhs.exp + adj)
        }
    }
}

impl Div for BigNum {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        if rhs == BigNum::ZERO {
            panic!("Attempt to divide by zero")
        }
        if rhs > self {
            // Division will result in 0
            return BigNum::ZERO;
        }
        if rhs == self {
            // Division will result in 1
            return BigNum::ONE;
        }

        let lhs_n = (self.base as u128) << 64;
        let rhs_n = rhs.base as u128;

        let result = lhs_n / rhs_n;
        let max_pow = utils::get_exp_u128(result) as u64;

        if self.exp != 0 {
            // Since self is in expanded form, when dividing by 1 we expect result's max_pow
            // to be 127 (64 + 63), if not we adjust self.exp (or base if res is compact)
            let adj = 127 - max_pow;
            // Since you normally adjust by 64, shift is 64 with adjustment
            let shift = 64 - adj;

            if adj >= self.exp {
                // Result can be made compact
                // If we want to adjust by 3 but self.exp is 1, we subtract 1 from adj
                // and then shift result by 2 (3 - 1)
                BigNum::new((result >> (64 - self.exp + rhs.exp)) as u64, 0)
            } else {
                // Result is expanded
                BigNum::new((result >> shift) as u64, self.exp - rhs.exp - adj)
            }
        } else {
            // self is compact so result must be compact
            BigNum::new((result >> 64) as u64, 0)
        }
    }
}

impl Sum for BigNum {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(BigNum::ZERO, |acc, x| acc + x)
    }
}

impl Product for BigNum {
    fn product<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(BigNum::ONE, |acc, x| acc * x)
    }
}

impl From<u64> for BigNum {
    fn from(value: u64) -> Self {
        BigNum::new(value, 0)
    }
}

impl From<u32> for BigNum {
    fn from(value: u32) -> Self {
        BigNum::new(value as u64, 0)
    }
}

impl From<u16> for BigNum {
    fn from(value: u16) -> Self {
        BigNum::new(value as u64, 0)
    }
}

impl From<u8> for BigNum {
    fn from(value: u8) -> Self {
        BigNum::new(value as u64, 0)
    }
}

impl From<i64> for BigNum {
    fn from(value: i64) -> Self {
        BigNum::new(value as u64, 0)
    }
}

impl From<i32> for BigNum {
    fn from(value: i32) -> Self {
        BigNum::new(value as u64, 0)
    }
}

impl From<i16> for BigNum {
    fn from(value: i16) -> Self {
        BigNum::new(value as u64, 0)
    }
}

impl From<i8> for BigNum {
    fn from(value: i8) -> Self {
        BigNum::new(value as u64, 0)
    }
}

impl BigNumConvertable for u64 {}
impl BigNumConvertable for u32 {}
impl BigNumConvertable for u16 {}
impl BigNumConvertable for u8 {}
impl BigNumConvertable for i64 {}
impl BigNumConvertable for i32 {}
impl BigNumConvertable for i16 {}
impl BigNumConvertable for i8 {}

impl<T> Add<T> for BigNum
where
    T: BigNumConvertable,
{
    type Output = Self;

    fn add(self, rhs: T) -> Self::Output {
        self + rhs.into()
    }
}

impl AddAssign for BigNum {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl<T> AddAssign<T> for BigNum
where
    T: BigNumConvertable,
{
    fn add_assign(&mut self, rhs: T) {
        *self = *self + rhs.into();
    }
}

impl<T> Sub<T> for BigNum
where
    T: BigNumConvertable,
{
    type Output = Self;

    fn sub(self, rhs: T) -> Self::Output {
        self - rhs.into()
    }
}

impl SubAssign for BigNum {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl<T> SubAssign<T> for BigNum
where
    T: BigNumConvertable,
{
    fn sub_assign(&mut self, rhs: T) {
        *self = *self - rhs.into();
    }
}

impl<T> Mul<T> for BigNum
where
    T: BigNumConvertable,
{
    type Output = Self;

    fn mul(self, rhs: T) -> Self::Output {
        self * rhs.into()
    }
}

impl MulAssign for BigNum {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl<T> MulAssign<T> for BigNum
where
    T: BigNumConvertable,
{
    fn mul_assign(&mut self, rhs: T) {
        *self = *self * rhs.into();
    }
}

impl<T> Div<T> for BigNum
where
    T: BigNumConvertable,
{
    type Output = Self;

    fn div(self, rhs: T) -> Self::Output {
        self / rhs.into()
    }
}

impl DivAssign for BigNum {
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

impl<T> DivAssign<T> for BigNum
where
    T: BigNumConvertable,
{
    fn div_assign(&mut self, rhs: T) {
        *self = *self / rhs.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A1: BigNum = BigNum::ZERO;

    const B1: BigNum = BigNum::ONE;
    const B2: BigNum = BigNum {
        base: 0x8000_0000_0000_0000,
        exp: 0,
        invalidate: false,
    };
    const B3: BigNum = BigNum {
        base: 0xFFFF_FFFF_FFFF_FFFF,
        exp: 0,
        invalidate: false,
    };

    const C1: BigNum = BigNum {
        base: 0x8000_0000_0000_0000,
        exp: 10,
        invalidate: false,
    };
    const C2: BigNum = BigNum {
        base: 0x8000_0000_0000_0000,
        exp: 73,
        invalidate: false,
    };
    const C3: BigNum = BigNum {
        base: 0xFFFF_FFFF_FFFF_FFFF,
        exp: 120,
        invalidate: false,
    };
    const C4: BigNum = BigNum {
        base: 0xFFFF_FFFF_FFFF_FFFF,
        exp: 127000,
        invalidate: false,
    };
    const C5: BigNum = BigNum {
        base: u64::MAX,
        exp: u64::MAX,
        invalidate: false,
    };

    #[test]
    fn it_works() {}

    #[test]
    fn new_add() {
        // A
        assert_eq!(A1 + A1, BigNum::ZERO);

        // A + B -> B
        assert_eq!(A1 + B1, BigNum::ONE);

        // B
        assert_eq!(B1 + B2, BigNum::new(0x8000_0000_0000_0001, 0));

        // B + B -> C
        assert_eq!(B1 + B3, BigNum::new(0x8000_0000_0000_0000, 1));
        assert_eq!(B2 + B3, BigNum::new(0xBFFF_FFFF_FFFF_FFFF, 1));

        // C
        assert_eq!(C1 + C2, BigNum::new(0x8000_0000_0000_0001, 73));
        assert_eq!(C2 + C3, BigNum::new(0x8000_0000_0000_7FFF, 121));
        assert_eq!(C3 + C4, C4); // Too small to make a difference
        assert_eq!(C1 + C3, C3);
    }

    #[should_panic]
    #[test]
    fn add_panic() {
        let _ = C5 + C5;
    }

    #[test]
    fn add() {
        let a: BigNum = 1.into();
        let b: BigNum = 1000000000001u64.into();

        assert_eq!(a + b, 1000000000002u64.into());

        let c: BigNum = u64::MAX.into();
        assert_eq!(a + c, BigNum::new(0x8000_0000_0000_0000, 1));

        let d = BigNum::new(0x8000_0000_0000_0000, 1);
        let e: BigNum = 2.into();
        let f: BigNum = 4.into();
        assert_eq!(a + d, d);
        assert_eq!(d + e, BigNum::new(0x8000_0000_0000_0001, 1));
        assert_eq!(d + f, BigNum::new(0x8000_0000_0000_0002, 1));

        let g = BigNum::new(0x8000_0000_0000_0000, 10000);
        let h = BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 9937);
        assert_eq!(g + h, BigNum::new(0x8000_0000_0000_0001, 10000));

        let i = BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 10000);
        let j = BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 9937);
        assert_eq!(i + j, BigNum::new(0x8000_0000_0000_0000, 10001));

        let k = BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 10000);
        let l = BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 10000);
        assert_eq!(k + l, BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 10001));
    }

    #[test]
    fn sub() {
        let a: BigNum = 1000000000001u64.into();
        let b: BigNum = 1.into();

        assert_eq!(a - b, 1000000000000u64.into());

        let c: BigNum = 0x8000_0000_0000_0000u64.into();
        assert_eq!(c - b, 0x7FFF_FFFF_FFFF_FFFFu64.into());

        let d = BigNum::new(0x8000_0000_0000_0000, 1);
        let e: BigNum = 2.into();
        let f: BigNum = 4.into();
        assert_eq!(d - b, BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 0));
        assert_eq!(d - e, BigNum::new(0xFFFF_FFFF_FFFF_FFFE, 0));
        assert_eq!(d - f, BigNum::new(0xFFFF_FFFF_FFFF_FFFC, 0));

        let g = BigNum::new(0x8000_0000_0000_0001, 10000);
        let h = BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 9937);
        assert_eq!(g - h, BigNum::new(0x8000_0000_0000_0000, 10000));

        assert_eq!(a - a, 0u64.into());
        assert_eq!(b - b, 0u64.into());
        assert_eq!(c - c, 0u64.into());
        assert_eq!(d - d, 0u64.into());
        assert_eq!(e - e, 0u64.into());
        assert_eq!(f - f, 0u64.into());
        assert_eq!(g - g, 0u64.into());
        assert_eq!(h - h, 0u64.into());
    }

    #[should_panic]
    #[test]
    fn sub_overflow() {
        let a: BigNum = 1.into();
        let b: BigNum = 2.into();

        let _ = a - b;
    }

    //#[test]
    //fn mul_u16() {
    //    let a = 1u16;
    //    let b = u16::MAX;
    //    let c = BigNum::new(MIN_BASE_VAL, 1);
    //
    //    assert_eq!(c * a, c);
    //    assert_eq!(c * b, BigNum::new(0xFFFF_0000_0000_0000, 16));
    //
    //    let d = BigNum::from(0x8000_1000_1000_1000u64);
    //    let e = 2u16;
    //    assert_eq!(d * e, BigNum::new(0x8000_1000_1000_1000, 1));
    //
    //    let f = 3u16;
    //    assert_eq!(d * f, BigNum::new(0xC000_1800_1800_1800, 1));
    //
    //    let g = BigNum::new(0x8000_1000_1000_1000, 100);
    //    assert_eq!(g * f, BigNum::new(0xC000_1800_1800_1800, 101));
    //}

    #[test]
    fn mul_u64() {
        let a = 1u64;
        let b = 0xFFFF_FFFF_FFFF_FFFFu64;
        let c = BigNum::new(MIN_BASE_VAL, 1);

        assert_eq!(c * a, c);
        assert_eq!(c * b, BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 64));

        let d = BigNum::from(0x8000_1000_1000_1000u64);
        let e = 2u64;
        assert_eq!(d * e, BigNum::new(0x8000_1000_1000_1000, 1));

        let f = 3u64;
        assert_eq!(d * f, BigNum::new(0xC000_1800_1800_1800, 1));

        let g = BigNum::new(0x8000_1000_1000_1000, 100);
        assert_eq!(g * f, BigNum::new(0xC000_1800_1800_1800, 101));
    }

    #[test]
    fn div_u64() {
        let a = 1u64;
        let b = 0x8000u64;
        let c = BigNum::new(MIN_BASE_VAL, 1);

        assert_eq!(c / a, c);
        assert_eq!(c / b, BigNum::new(0x0002_0000_0000_0000, 0));

        let d = BigNum::from(0x8000_1000_1000_1000u64);
        let e = 2u64;
        assert_eq!(d / e, BigNum::new(0x4000_0800_0800_0800, 0));

        //let d = BigNum::from(1e19)
        // d is right above lower limit for base
        let f = BigNum::new(10_000_000_000_000_000_000u64, 1);
        let g = 5u64;
        assert_eq!(f / g, BigNum::from(4_000_000_000_000_000_000u64));

        let h = BigNum::new(MIN_BASE_VAL, 10000);
        let i = BigNum::new(MIN_BASE_VAL, 9937);
        let j = BigNum::new(MIN_BASE_VAL, 9936);
        assert_eq!(h / i, BigNum::new(MIN_BASE_VAL, 0));
        assert_eq!(h / j, BigNum::new(MIN_BASE_VAL, 1));

        assert_eq!(c / c, BigNum::ONE);
        assert_eq!(d / d, BigNum::ONE);
        assert_eq!(f / f, BigNum::ONE);
        assert_eq!(h / h, BigNum::ONE);
        assert_eq!(i / i, BigNum::ONE);
        assert_eq!(j / j, BigNum::ONE);
    }

    #[should_panic]
    #[test]
    fn div_zero() {
        let _ = BigNum::ONE / BigNum::ZERO;
    }
}
