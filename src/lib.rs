//! This crate defines a custom medium-precision number type. It can support any base `b`
//! in the range `[0, 65535]`, and can approximately represent numbers up to
//! `b ^ u64::MAX` (actually a bit higher than that but the math is complicated). A core
//! goal for this type was that it can implement `Copy` and as a result it can be used in
//! almost any context a normal unsigned integer would be valid.

// public re-exporting
#[cfg(feature = "macro")]
pub use bignum_proc_macro::{create_efficient_base, make_bignum};

use std::{
    cmp::Ordering,
    fmt::{Debug, Display},
    iter::{Product, Sum},
    ops::{Add, AddAssign, Div, Mul, MulAssign, Shl, Shr, Sub, SubAssign},
};

use consts::{
    BIN_EXP_RANGE, BIN_POWERS, BIN_POWERS_U128, BIN_SIG_RANGE, DEC_EXP_RANGE, DEC_POWERS,
    DEC_POWERS_U128, DEC_SIG_RANGE, HEX_EXP_RANGE, HEX_POWERS, HEX_POWERS_U128, HEX_SIG_RANGE,
    OCT_EXP_RANGE, OCT_POWERS, OCT_POWERS_U128, OCT_SIG_RANGE,
};

#[cfg(any(feature = "random", test))]
pub mod random;

pub(crate) mod consts;
pub(crate) mod macros;

pub mod traits;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// This represents the non-inclusive range of exponents that constitute a valid
/// non-compact significand in the given base. You only need to use this if manually
/// defining a custom base (if performance is non-critical I would recommend using the
/// `create_default_base` macro).
///
/// # Examples
/// ```
/// use bignumbe_rs::{ExpRange, Binary, Base};
///
/// let ExpRange(min_exp, max_exp) = Binary::calculate_ranges().0;
///
/// // Since the range of valid significands for non-compact Binary BigNum instances is
/// // [2^63, 2^64), we expect an ExpRange of (63, 64)
/// assert_eq!(min_exp, 63);
/// assert_eq!(max_exp, 64);
/// ```
pub struct ExpRange(pub u32, pub u32);

impl ExpRange {
    pub const fn new(min: u32, max: u32) -> Self {
        Self(min, max)
    }

    pub const fn from(range: (u32, u32)) -> Self {
        Self(range.0, range.1)
    }

    pub fn min(&self) -> u32 {
        self.0
    }

    pub fn max(&self) -> u32 {
        self.1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// This represents the inclusive range of values that constitute a valid non-compact
/// significand in the given base. You only need to use this if manually defining a custom
/// base (if performance is non-critical I would recommend using the `create_default_base`
/// macro).
///
/// # Examples
/// ```
/// use bignumbe_rs::{SigRange, Binary, Base};
///
/// let SigRange(min_sig, max_sig) = Binary::calculate_ranges().1;
///
/// // Since the range of valid significands for non-compact Binary BigNum instances is
/// // [2^63, 2^64), we expect a SigRange of (2^63, 2^64 - 1)
/// assert_eq!(min_sig, 1 << 63);
/// assert_eq!(max_sig, u64::MAX);
/// ```
pub struct SigRange(pub u64, pub u64);

impl SigRange {
    pub const fn new(min: u64, max: u64) -> Self {
        Self(min, max)
    }

    pub const fn from(range: (u64, u64)) -> Self {
        Self(range.0, range.1)
    }

    pub fn min(&self) -> u64 {
        self.0
    }

    pub fn max(&self) -> u64 {
        self.1
    }
}

/// If performance isn't critical I'd highly recommend the `create_default_base` macro
/// which creates a base with sensible defaults. The only reason to create a custom
/// implementation is if you find the default implementations' operations to be a
/// bottleneck. In this case I'd recommend looking at my implementation of the `Decimal`
/// base as a guide.
///
/// This trait is used to indicate that a type is a valid base for a BigNumBase. It
/// contains metadata and functions that can be used to efficiently handle arbitrary
/// bases. Importantly you must ensure all of the following:
/// - `base.exp_range().max() = base.exp_range().min() + 1`
/// - `base.exp_range().min() > 0`
/// - `base.sig_range().min() = base.pow(exp_range().min())`
/// - `base.sig_range().max() = base.pow(exp_range().max()) - 1`
/// - `B::pow(n) = NUMBER.pow(n)` for all `n < base.exp_range().max()`
/// - `B::rshift(lhs, exp) = lhs / B::NUMBER.pow(n)` for all `n <= base.exp_range().max()`
/// - `B::lshift(lhs, exp) = lhs * B::NUMBER.exp(n)` for all
///     `n <= base.exp_range().max()`
/// - `B::get_mag(n)` should return the highest exponent `x` such that `n >= B::pow(x)`,
///     for all `n <= exp_range().max()`
/// - `base.sig_range().min() * B::NUMBER > u64::MAX`
///     - This restriction allows us to conveniently handle some construction cases
///
/// The above requirements also hold for the `u128` versions of
/// `lshift, rshift, get_mag, pow` which are used for multiplication and division (since
/// those calculations involve projecting values to `u128` to preserve information)
///
/// Some of these calculations have the potential to overflow a `u64` so you may need to
/// think of other ways to compute them if you plan to verify them manually.
///
/// Additionally, the implementers will be copied on every math operation and in some
/// other contexts, so ensure that they are lightweight. E.g. even though
/// ```
/// #[derive(Clone, Copy, Debug)]
/// pub struct CustomBase {
///     metadata: [u8; 10000000000],
/// }
/// ```
/// is valid, it's ill-advised here. If you need a table of powers I would recommend a
/// global const array that you reference in the `pow` method.
///
/// The recommended format for
/// a non-performance critical simple Base definition and implementation is:
/// ```
/// use bignumbe_rs::{ExpRange, SigRange, Base};
///
/// #[derive(Clone, Copy, Debug)]
/// pub struct Base13 {
///     exp_range: ExpRange,
///     sig_range: SigRange
/// }
///
/// impl Base for Base13{
///     const NUMBER: u16 = 13;
///
///     fn new() -> Self {
///         let (exp_range, sig_range) = Self::calculate_ranges();
///         Self {exp_range, sig_range}
///     }
///
///     fn exp_range(&self) -> ExpRange {
///         self.exp_range
///     }
///
///     fn sig_range(&self) -> SigRange {
///         self.sig_range
///     }
/// }
/// ```
pub trait Base: Copy + Debug {
    /// This contains the numeric value of the type. E.g. for binary 2, for decimal 10,
    /// etc.
    const NUMBER: u16;

    /// Function that can create an instance of this Base. Users should never have to
    /// manually create instances of this type. This is called implicitly on every
    /// call to `BigNumBase<Self>::new()` so it should be as lightweight as possible. Note
    /// that it is not called when creating a BigNumBase<Self> from another, like when
    /// performing an addition. In this case the base is simply copied over.
    fn new() -> Self;

    /// Function that fetches the non-inclusive range of the exponent for the significand
    /// in the BigNum with this base. E.g. the range for binary is [63, 64), since the
    /// range of the significand is [2^63, 2^64)
    fn exp_range(&self) -> ExpRange;

    /// Function that fetches the inclusive range for the significand in the BigNum with
    /// this base. E.g. for binary the range of the significand is [2^63, 2^64 - 1]
    fn sig_range(&self) -> SigRange;

    /// This is a function that computes `Self::NUMBER ^ exp`. It has a default
    /// implementation that computes the value directly. It is recommended to override
    /// this behavior if there is a trick to the exponentiation (like how for binary
    /// `2^n = (1 << n)`). You can also create a gloabl const lookup table and reference
    /// that.
    fn pow(exp: u32) -> u64 {
        (Self::NUMBER as u64).pow(exp)
    }

    /// This is a function that computes the same value as `pow` but in a u128 value.
    /// Mostly useful to help with multiplication/division, and as such it's probably
    /// unnecessary to override it unless multiplication/division performance is critical
    fn pow_u128(exp: u32) -> u128 {
        (Self::NUMBER as u128).pow(exp)
    }

    /// This function calculates the ranges for the exponent and the significand. It is
    /// not particularly efficient so if performance is a concern you should not use it.
    /// It mainly exists to facilitate the `create_default_base!` macro. It is recommended
    /// to store the ranges in a const and return them directly in the `exp_range` and
    /// `sig_range` methods if convenient.
    fn calculate_ranges() -> (ExpRange, SigRange) {
        if Self::NUMBER.is_power_of_two() && Self::NUMBER.ilog2().is_power_of_two() {
            // This is a special case where sig_max = u64::MAX. We have to handle it
            // specially to avoid overflowing the u64
            let pow = Self::NUMBER.ilog2();
            let exp = 64 / pow;
            let sig = Self::pow(exp - 1);

            (ExpRange(exp - 1, exp), SigRange(sig, u64::MAX))
        } else {
            let exp = u64::MAX.ilog(Self::NUMBER as u64);
            (
                ExpRange(exp - 1, exp),
                SigRange(Self::pow(exp - 1), Self::pow(exp) - 1),
            )
        }
    }

    /// This is a function that computes `lhs * (Self::NUMBER ^ exp)`. There is a default
    /// implementation that obtains the value of `Self::NUMBER ^ exp` via the `pow` method
    /// for this type, and does a division. It is recommended to override this method if
    /// there is a trick for the division (like how in binary,
    /// `lhs * (2 ^ exp) = lhs >> exp`, or in octal `lhs * (8 ^ exp) = lhs >> (3 * exp)`
    fn lshift(lhs: u64, exp: u32) -> u64 {
        lhs * Self::pow(exp)
    }
    /// This is a function that computes `lhs / (Self::NUMBER ^ exp)`. There is a default
    /// implementation that obtains the value of `Self::NUMBER ^ exp` via the `pow` method
    /// for this type, and does a multiplication. It is recommended to override this
    /// method if there is a trick for the division (like how in binary,
    /// `lhs / (2 ^ exp) = lhs << exp`, or in octal `lhs / (8 ^ exp) = lhs << (3 * exp)`
    fn rshift(lhs: u64, exp: u32) -> u64 {
        lhs / Self::pow(exp)
    }

    /// This is a function that computes the same thing as `lshift` but in a u128 value.
    /// Mostly useful to help with multiplication/division, and as such it's probably
    /// unnecessary to override it unless multiplication/division performance is critical
    fn lshift_u128(lhs: u128, exp: u32) -> u128 {
        lhs * Self::pow_u128(exp)
    }

    /// This is a function that computes the same thing as `rshift` but in a u128 value.
    /// Mostly useful to help with multiplication/division, and as such it's probably
    /// unnecessary to override it unless multiplication/division performance is critical
    fn rshift_u128(lhs: u128, exp: u32) -> u128 {
        lhs / Self::pow_u128(exp)
    }

    /// This is a function that computes the highest power `x` such that
    /// `sig >= (Self::NUMBER ^ x)`. There is a default implementation that uses `ilog`,
    /// and it is recommended to use this unless there is a special way to find the
    /// magnitude (e.g. binary and decimal have specialized `ilog` implementations).
    /// As a special case, bases that are powers of 2 or 10 can use log arithmetic to
    /// convert. I tried this with octal and hexadecimal but it had no noticeable impact.
    fn get_mag(sig: u64) -> u32 {
        sig.ilog(Self::NUMBER as u64)
    }

    /// This is a function that computes the same thing as `get_mag` but in a u128 value.
    /// Mostly useful to help with multiplication/division, and as such it's probably
    /// unnecessary to override it unless multiplication/division performance is critical
    fn get_mag_u128(sig: u128) -> u32 {
        sig.ilog(Self::NUMBER as u128)
    }

    /// This method just fetches `Self::NUMBER` but is provided as an instance method for
    /// convenience. Overriding it is undefined behavior
    fn as_number(&self) -> u16 {
        Self::NUMBER
    }
}

/// This type represents a binary base. It contains more efficient overrides of the
/// `Base` functions to improve performance.
#[derive(Clone, Copy, Debug)]
pub struct Binary;
pub type BigNumBin = BigNumBase<Binary>;

/// This type represents an octal base. It contains more efficient overrides of the
/// `Base` functions to improve performance.
#[derive(Clone, Copy, Debug)]
pub struct Octal;
pub type BigNumOct = BigNumBase<Octal>;

/// This type represents a hexadecimal base. It contains more efficient overrides of the
/// `Base` functions to improve performance.
#[derive(Clone, Copy, Debug)]
pub struct Hexadecimal;
pub type BigNumHex = BigNumBase<Hexadecimal>;

/// This type represents a decimal base. It contains more efficient overrides of the
/// `Base` functions to improve performance.
#[derive(Clone, Copy, Debug)]
pub struct Decimal;
pub type BigNumDec = BigNumBase<Decimal>;

impl Base for Binary {
    const NUMBER: u16 = 2;
    fn new() -> Self {
        Self
    }

    fn exp_range(&self) -> ExpRange {
        //ExpRange(63, 64)
        ExpRange::from(BIN_EXP_RANGE)
    }

    fn sig_range(&self) -> SigRange {
        //SigRange(1 << 63, u64::MAX)
        SigRange::from(BIN_SIG_RANGE)
    }

    fn pow(exp: u32) -> u64 {
        BIN_POWERS[exp as usize]
    }

    fn pow_u128(exp: u32) -> u128 {
        BIN_POWERS_U128[exp as usize]
    }

    fn rshift(lhs: u64, exp: u32) -> u64 {
        lhs >> exp
    }

    fn rshift_u128(lhs: u128, exp: u32) -> u128 {
        lhs >> exp
    }

    fn lshift(lhs: u64, exp: u32) -> u64 {
        lhs << exp
    }

    fn lshift_u128(lhs: u128, exp: u32) -> u128 {
        lhs << exp
    }

    fn get_mag(sig: u64) -> u32 {
        sig.ilog2()
    }

    fn get_mag_u128(sig: u128) -> u32 {
        sig.ilog2()
    }
}

impl Base for Octal {
    const NUMBER: u16 = 8;

    fn new() -> Self {
        Self
    }

    fn exp_range(&self) -> ExpRange {
        ExpRange::from(OCT_EXP_RANGE)
    }

    fn sig_range(&self) -> SigRange {
        SigRange::from(OCT_SIG_RANGE)
    }

    fn pow(exp: u32) -> u64 {
        OCT_POWERS[exp as usize]
    }

    fn pow_u128(exp: u32) -> u128 {
        OCT_POWERS_U128[exp as usize]
    }

    fn rshift(lhs: u64, exp: u32) -> u64 {
        lhs >> (3 * exp)
    }

    fn rshift_u128(lhs: u128, exp: u32) -> u128 {
        lhs >> (3 * exp)
    }

    fn lshift(lhs: u64, exp: u32) -> u64 {
        lhs << (3 * exp)
    }

    fn lshift_u128(lhs: u128, exp: u32) -> u128 {
        lhs << (3 * exp)
    }
}

impl Base for Hexadecimal {
    const NUMBER: u16 = 16;

    fn new() -> Self {
        Self
    }

    fn exp_range(&self) -> ExpRange {
        ExpRange::from(HEX_EXP_RANGE)
    }

    fn sig_range(&self) -> SigRange {
        SigRange::from(HEX_SIG_RANGE)
    }

    fn pow(exp: u32) -> u64 {
        HEX_POWERS[exp as usize]
    }

    fn pow_u128(exp: u32) -> u128 {
        HEX_POWERS_U128[exp as usize]
    }

    fn lshift(lhs: u64, exp: u32) -> u64 {
        lhs << (4 * exp)
    }

    fn lshift_u128(lhs: u128, exp: u32) -> u128 {
        lhs << (4 * exp)
    }

    fn rshift(lhs: u64, exp: u32) -> u64 {
        lhs >> (4 * exp)
    }

    fn rshift_u128(lhs: u128, exp: u32) -> u128 {
        lhs >> (4 * exp)
    }
}

impl Base for Decimal {
    const NUMBER: u16 = 10;

    fn new() -> Self {
        Self
    }

    fn exp_range(&self) -> ExpRange {
        ExpRange(DEC_EXP_RANGE.0, DEC_EXP_RANGE.1)
    }

    fn sig_range(&self) -> SigRange {
        SigRange(DEC_SIG_RANGE.0, DEC_SIG_RANGE.1)
    }

    fn pow(exp: u32) -> u64 {
        DEC_POWERS[exp as usize]
    }

    fn pow_u128(exp: u32) -> u128 {
        DEC_POWERS_U128[exp as usize]
    }

    fn get_mag(sig: u64) -> u32 {
        sig.ilog10()
    }

    fn get_mag_u128(sig: u128) -> u32 {
        sig.ilog10()
    }
}

/// This is the main struct for `bignumbe-rs`.
///
/// It takes a generic argument for the base, e.g.
/// `BigNumBase<Binary>`. It is recommended to either create a custom type alias or
/// use one of the predefined ones (`BigNumBin, BigNumOct, BigNumDec, BigNumHex`). You
/// should be able to use them pretty much exactly like other numbers in most contexts.
/// For convenience I define `From` and all math operations for `u64`, but keep in mind
/// that the `From` implementation, like `new`, involves recalculating the base ranges.
///
/// ```
/// use bignumbe_rs::{BigNumBase, Binary};
///
/// type BigNum = BigNumBase<Binary>;
///
/// let bn1 = BigNum::from(1);
/// let bn2 = BigNum::from(u64::MAX);
///
/// // Since this operation's result doesn't fit in `u64` it wraps over to the minimum
/// // significand and increments the `exp`
/// assert_eq!(bn1 + bn2, BigNum::new(1 << 63, 1));
///
/// assert_eq!(bn1 / bn2, BigNum::from(0));
/// assert_eq!(bn1 * bn2, bn2);
/// assert_eq!(bn2 * bn2, BigNum::new(u64::MAX - 1, 64));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct BigNumBase<T>
where
    T: Base,
{
    pub sig: u64,
    pub exp: u64,
    pub base: T,
}

impl<T> BigNumBase<T>
where
    T: Base,
{
    /// Creates a new `BigNumBase` instance that represents the value
    /// `sig * T::NUMBER^exp`. E.g. `BigNumBin::new(12341234, 12341)` represents
    /// `12341234 * 2^12341`. This method will perform normalization if necessary, to
    /// ensure the significand is in the valid range (if the number is non-compact). As
    /// such when creating a BigNum from scratch you should always use this unless you
    /// absolutely need a raw constructor
    pub fn new(sig: u64, exp: u64) -> Self {
        let base = T::new();

        let SigRange(min_sig, max_sig) = base.sig_range();
        let ExpRange(min_exp, _) = base.exp_range();

        if sig >= min_sig && sig <= max_sig {
            Self { sig, exp, base }
        } else if sig > max_sig {
            // Since we know `max_sig * base.as_number() > u64::MAX`, we also know
            // that `sig / base.as_number() <= max_sig`
            Self {
                sig: T::rshift(sig, 1),
                exp: exp.checked_add(1).unwrap_or_else(|| {
                    panic!(
                        "Unable to create a BigNum with an exp of u64::MAX and a significand greater than max_sig = {}",
                        max_sig
                    )
                }),
                base,
            }
        } else if exp == 0 {
            Self { sig, exp, base }
        } else if sig == 0 {
            panic!(
                "Unable to create BigNumBase with exp of {} and sig of 0",
                exp
            );
        } else {
            let mag = T::get_mag(sig);

            if mag.saturating_add(exp as u32) <= min_exp {
                Self {
                    sig: T::lshift(sig, exp as u32),
                    exp: 0,
                    base,
                }
            } else {
                let adj = min_exp - mag;

                Self {
                    sig: T::lshift(sig, adj),
                    exp: exp - adj as u64,
                    base,
                }
            }
        }
    }

    /// Creates a BigNumBase directly from values, panicking if not possible. This is
    /// mostly for testing but may be more performant on inputs that are guaranteed valid
    pub fn new_raw(sig: u64, exp: u64) -> Self {
        let base = T::new();

        if Self::is_valid(sig, exp, base.sig_range()) {
            Self { sig, exp, base }
        } else {
            panic!(
                "Unable to create BigNumBase with sig 
0x{:x} and exp 
{}
min_sig:
0x{:x},
max_sig:
0x{:x}",
                sig,
                exp,
                base.sig_range().0,
                base.sig_range().1
            );
        }
    }

    /// Returns true if the values are valid for the current base
    fn is_valid(sig: u64, exp: u64, range: SigRange) -> bool {
        sig <= range.max() && (exp == 0 || sig >= range.min())
    }

    /// Allows fuzzy comparison between two values. Since operations can result in loss of
    /// precision this allows you to compare values that may have drifted. Since each
    /// operation can result in an error of 1, an upper bound is the sum of the number of
    /// operations performed on each operand. E.g. for `n: BigNumDec`, to ensure that
    /// (n * 1000) / 500 = (n / 500) * 1000, you might use a margin of 4
    pub fn fuzzy_eq(self, other: Self, margin: u64) -> bool {
        let SigRange(min_sig, max_sig) = self.base.sig_range();

        let (min, max) = if self > other {
            (other, self)
        } else {
            (self, other)
        };

        if max.exp == min.exp {
            max.sig - min.sig <= margin
        } else if max.exp == min.exp.wrapping_add(1) {
            max.sig.saturating_sub(margin) <= min_sig && min.sig.saturating_add(margin) >= max_sig
        } else {
            false
        }
    }
}

impl<T> PartialEq for BigNumBase<T>
where
    T: Base,
{
    fn eq(&self, other: &Self) -> bool {
        self.sig == other.sig && self.exp == other.exp
    }
}

impl<T> Eq for BigNumBase<T> where T: Base {}

impl<T> Ord for BigNumBase<T>
where
    T: Base,
{
    fn cmp(&self, other: &Self) -> Ordering {
        match self.exp.cmp(&other.exp) {
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
            Ordering::Equal => match self.sig.cmp(&other.sig) {
                Ordering::Less => Ordering::Less,
                Ordering::Greater => Ordering::Greater,
                Ordering::Equal => Ordering::Equal,
            },
        }
    }
}

impl<T> PartialOrd for BigNumBase<T>
where
    T: Base,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Add for BigNumBase<T>
where
    T: Base,
{
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let base = self.base;
        let SigRange(min_sig, max_sig) = base.sig_range();
        let ExpRange(_, max_exp) = base.exp_range();

        let (max, min) = if self > rhs { (self, rhs) } else { (rhs, self) };
        let shift = max.exp - min.exp;

        if shift >= max_exp as u64 {
            // This shift is guaranteed to result in 0 on lhs, no need to compute
            return max;
        }

        let result = max.sig.wrapping_add(T::rshift(min.sig, shift as u32));

        let (sig, exp) = if result < max.sig {
            // How much we need to add to the overflow result to make up for differences
            // in the significand's range
            let diff = u64::MAX - max_sig;
            (min_sig + T::rshift(result + diff, 1), max.exp + 1)
        } else if T::NUMBER != 2 && result > max_sig {
            (T::rshift(result, 1), max.exp + 1)
        } else {
            (result, max.exp)
        };

        Self {
            sig,
            exp,
            base: self.base,
        }
    }
}

impl<T> AddAssign for BigNumBase<T>
where
    T: Base,
{
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl<T> Sub for BigNumBase<T>
where
    T: Base,
{
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let base = self.base;
        let SigRange(min_sig, _) = base.sig_range();
        let ExpRange(min_exp, max_exp) = base.exp_range();

        let (max, min) = if self >= rhs {
            (self, rhs)
        } else {
            panic!(
                "Attempt to subtract 
{:?} from 
{:?}",
                rhs, self
            )
        };

        let shift = max.exp - min.exp;

        if shift >= max_exp as u64 {
            // This shift is guaranteed to result in 0 on rhs, no need to compute
            return max;
        }

        let result = max.sig.wrapping_sub(T::rshift(min.sig, shift as u32));

        let (res_sig, res_exp) = if result > max.sig {
            // Wrapping occurred, handle it by decrementing the exponent
            (result, max.exp - 1)
        } else {
            (result, max.exp)
        };

        if res_sig == 0 {
            Self {
                sig: 0,
                exp: 0,
                base,
            }
        } else if res_exp == 0 || res_sig >= min_sig {
            Self {
                sig: res_sig,
                exp: res_exp,
                base,
            }
        } else {
            // This operation can result in arbitrary loss in magnitude so we have to
            // calculate the differential directly
            let mag = T::get_mag(res_sig);
            let adj = min_exp - mag;

            if adj as u64 == res_exp {
                Self {
                    sig: T::lshift(res_sig, adj),
                    exp: 0,
                    base,
                }
            } else if adj as u64 >= res_exp {
                // Have to adjust by more than exp so we will have a compact result
                // TODO Verify this again, pretty sure it's right but I can't figure out
                // why the -1 is there
                let diff = adj as u64 - res_exp - 1;

                Self {
                    sig: T::lshift(res_sig, diff as u32),
                    exp: 0,
                    base,
                }
            } else {
                Self {
                    sig: T::lshift(res_sig, adj),
                    exp: res_exp - adj as u64,
                    base,
                }
            }
        }
    }
}

impl<T> SubAssign for BigNumBase<T>
where
    T: Base,
{
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl<T> Mul for BigNumBase<T>
where
    T: Base,
{
    type Output = BigNumBase<T>;

    fn mul(self, rhs: Self) -> Self::Output {
        let base = self.base;

        if self.exp == 0 && self.sig == 1 {
            return rhs;
        } else if self.exp == 0 && self.sig == 0 {
            return Self {
                sig: 0,
                exp: 0,
                base,
            };
        } else if rhs.exp == 0 && rhs.sig == 1 {
            return self;
        } else if rhs.exp == 0 && rhs.sig == 0 {
            return Self {
                sig: 0,
                exp: 0,
                base,
            };
        }

        let (lsig, rsig) = (self.sig as u128, rhs.sig as u128);
        let (lexp, rexp) = (self.exp, rhs.exp);
        let SigRange(min_sig, max_sig) = base.sig_range();
        let ExpRange(min_exp, _) = base.exp_range();

        let res_sig = lsig * rsig;
        let res_exp = lexp + rexp;

        if res_sig > max_sig as u128 {
            let mag = T::get_mag_u128(res_sig);

            let adj = mag - min_exp;
            let sig = T::rshift_u128(res_sig, adj);
            if sig > u64::MAX as u128 {
                panic!(
                    "Unable to normalize result for multiplication between {:?} and {:?}",
                    self, rhs
                );
            } else {
                Self {
                    sig: sig as u64,
                    exp: res_exp + adj as u64,
                    base,
                }
            }
        } else if res_exp != 0 && res_sig < min_sig as u128 {
            panic!(
                "Found invalid significand while multiplying {:?} and {:?}",
                self, rhs
            );
        } else {
            Self {
                sig: res_sig as u64,
                exp: res_exp,
                base,
            }
        }
    }
}

impl<T> MulAssign for BigNumBase<T>
where
    T: Base,
{
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl<T> Div for BigNumBase<T>
where
    T: Base,
{
    type Output = Self;

    // The basic idea here is to project both numbers to a u128 like in multiplication,
    // but this time the lhs goes in the upper 64 bits and the rhs goes in the lower. This
    // way we preserve as much info as possible
    fn div(self, rhs: Self) -> Self::Output {
        match self.cmp(&rhs) {
            Ordering::Less => return Self::new(0, 0),
            Ordering::Equal => return Self::new(1, 0),
            _ => (),
        }

        if self.exp == 0 {
            return Self {
                sig: self.sig / rhs.sig,
                ..self
            };
        }

        let base = self.base;
        let ExpRange(min_exp, max_exp) = base.exp_range();

        let (lsig, rsig) = (T::lshift_u128(self.sig as u128, max_exp), rhs.sig as u128);
        let (lexp, rexp) = (self.exp, rhs.exp);

        let res_sig = lsig / rsig;
        let res_exp = lexp - rexp;

        let mag = T::get_mag_u128(res_sig);
        // lsig had a magnitude of min_exp + max_exp, this tracks how many orders of
        // magnitude were "lost" with this division
        let adj = (min_exp + max_exp) - mag;

        if adj as u64 <= res_exp {
            // We would shift by max_exp normally, but since we lost adj orders of
            // magnitude we have to shift by max_exp - adj
            Self {
                sig: T::rshift_u128(res_sig, max_exp - adj) as u64,
                exp: res_exp - adj as u64,
                ..self
            }
        } else {
            let diff = adj as u64 - res_exp;
            // We would normally shift by max_exp, but we lost adj order of magnitude
            // and took diff orders of magnitude from the exponent, so we shift by
            // max_exp - adj + diff
            Self {
                sig: T::rshift_u128(res_sig, max_exp - adj + diff as u32) as u64,
                exp: 0,
                ..self
            }
        }
    }
}

impl<T> Shl<u64> for BigNumBase<T>
where
    T: Base,
{
    type Output = Self;

    fn shl(self, rhs: u64) -> Self::Output {
        let ExpRange(min_exp, _) = self.base.exp_range();

        if self.exp != 0 {
            // Already in expanded form
            Self {
                exp: self.exp.checked_add(rhs).unwrap(),
                ..self
            }
        } else {
            let mag = T::get_mag(self.sig);
            // The number of orders of magnitude the significand can be increased
            let adj = min_exp - mag;

            if adj as u64 > rhs {
                // The result can be made compact
                Self {
                    sig: T::lshift(self.sig, rhs as u32),
                    exp: 0,
                    ..self
                }
            } else {
                Self {
                    sig: T::lshift(self.sig, adj),
                    exp: rhs - adj as u64,
                    ..self
                }
            }
        }
    }
}

impl<T> Shr<u64> for BigNumBase<T>
where
    T: Base,
{
    type Output = Self;

    fn shr(self, rhs: u64) -> Self::Output {
        if self.exp >= rhs {
            return Self {
                exp: self.exp - rhs,
                ..self
            };
        }

        let mag = T::get_mag(self.sig);
        let diff = rhs - self.exp;

        if diff > mag as u64 {
            panic!("Unable to shift {:?} by {}", self, rhs);
        }

        Self {
            sig: T::rshift(self.sig, diff as u32),
            exp: 0,
            ..self
        }
    }
}

impl<T> Sum for BigNumBase<T>
where
    T: Base,
{
    fn sum<I: Iterator<Item = Self>>(mut iter: I) -> Self {
        if let Some(elem) = iter.next() {
            iter.fold(elem, |acc, n| acc + n)
        } else {
            Self::from(0)
        }
    }
}

impl<T> Product for BigNumBase<T>
where
    T: Base,
{
    fn product<I: Iterator<Item = Self>>(mut iter: I) -> Self {
        if let Some(elem) = iter.next() {
            iter.fold(elem, |acc, n| acc * n)
        } else {
            Self::from(0)
        }
    }
}

impl Display for BigNumBase<Decimal> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.exp == 0 {
            // Precision specifier has special behavior on floats which is undesired
            // here. Want to force it to string and use the default behavior, e.g.
            // a max-width setting.
            let mag = Decimal::get_mag(self.sig);

            if mag < 3 {
                f.write_fmt(format_args!("{}", self.sig))
            } else if mag < 6 {
                f.write_fmt(format_args!("{0:.5}k", (self.sig as f64 / 1e3).to_string()))
            } else if mag < 9 {
                f.write_fmt(format_args!("{0:.5}m", (self.sig as f64 / 1e6).to_string()))
            } else if mag < 12 {
                f.write_fmt(format_args!("{0:.5}b", (self.sig as f64 / 1e9).to_string()))
            } else if mag < 15 {
                f.write_fmt(format_args!(
                    "{0:.5}t",
                    (self.sig as f64 / 1e12).to_string()
                ))
            } else {
                let res = (self.sig as f64) / 10f64.powi(mag as i32);

                if res == 10.0 {
                    f.write_fmt(format_args!("9.999e{}", mag))
                } else {
                    f.write_fmt(format_args!("{0:.5}e{1}", res.to_string(), mag))
                }
            }
        } else {
            let min_exp = self.base.exp_range().min();
            let res = (self.sig as f64) / 10f64.powi(min_exp as i32);

            if res == 10.0 {
                f.write_fmt(format_args!("9.999e{}", min_exp as u64 + self.exp))
            } else {
                f.write_fmt(format_args!(
                    "{0:.5}e{1}",
                    res.to_string(),
                    min_exp as u64 + self.exp
                ))
            }
        }
    }
}

impl<T> Mul<f64> for BigNumBase<T>
where
    T: Base,
{
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        let min_exp = self.base.exp_range().min();
        let cutoff_exp = min_exp / 2;
        let cutoff = T::pow(cutoff_exp);
        if rhs > cutoff as f64 {
            if rhs > u64::MAX as f64 {
                let mag = rhs.log(T::NUMBER as f64).floor() as u64;
                let diff = mag - min_exp as u64;

                self * Self::new((rhs / (T::NUMBER as f64).powi(diff as i32)) as u64, diff)
            } else {
                // Anything after the decimal point won't make a significant difference in
                // the total
                self * (rhs.ceil() as u64)
            }
        } else {
            (self * (rhs * cutoff as f64).ceil() as u64) / cutoff
        }
    }
}

impl<T> MulAssign<f64> for BigNumBase<T>
where
    T: Base,
{
    fn mul_assign(&mut self, rhs: f64) {
        *self = *self * rhs;
    }
}

#[cfg(test)]
mod tests {
    use std::iter::repeat_n;

    use macros::test_macros::assert_eq_bignum;
    use rand::distributions::Uniform;
    use rand::prelude::Distribution;
    use rand::thread_rng;
    use traits::Succ;

    use super::*;
    use crate::Binary;

    #[test]
    fn new_binary_test() {
        type BigNum = BigNumBase<Binary>;
        // Check that adjustment is correct, especially around edge cases
        assert_eq_bignum!(BigNum::new(1, 0), BigNum::new_raw(1, 0));
        assert_eq_bignum!(BigNum::new(0b100, 2), BigNum::new_raw(0b10000, 0));
        assert_eq_bignum!(BigNum::new(1 << 62, 20), BigNum::new_raw(1 << 63, 19));
        assert_eq_bignum!(BigNum::new(1 << 62, 20), BigNum::new_raw(1 << 63, 19));
    }

    #[test]
    fn add_binary_test() {
        type BigNum = BigNumBase<Binary>;
        assert_eq_bignum!(
            BigNum::new(0x100, 0) + BigNum::new(0x0100_0000, 4),
            BigNum::new_raw(0x1000_0100, 0)
        );
        assert_eq_bignum!(
            BigNum::new(0x1000_0000, 32) + BigNum::new(0x0100_0000, 4),
            BigNum::new_raw(0x1000_0000_1000_0000, 0)
        );
        assert_eq_bignum!(
            BigNum::new(0xFFFF_FFFF, 32) + BigNum::new(0x8000_0000, 1),
            BigNum::new_raw(0x8000_0000_0000_0000, 1)
        );
        assert_eq_bignum!(
            BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 1) + 0x1u64,
            BigNum::new_raw(0xFFFF_FFFF_FFFF_FFFF, 1)
        );
        assert_eq_bignum!(
            BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 1) + 0x2u64,
            BigNum::new_raw(0x8000_0000_0000_0000, 2)
        );
    }

    #[test]
    fn add_hex_test() {
        type BigNum = BigNumBase<Hexadecimal>;
        assert_eq_bignum!(
            BigNum::from(0xFFFF_FFFF_FFFF_FFFFu64) + 1u64,
            BigNum::new_raw(0x1000_0000_0000_0000, 1)
        );
        assert_eq_bignum!(
            BigNum::from(0xFFFF_FFFF_FFFF_FFFEu64) + 1u64,
            BigNum::new_raw(0xFFFF_FFFF_FFFF_FFFF, 0)
        );
        assert_eq_bignum!(
            BigNum::new(0xFFFF_FFFF_FFFF_FFFEu64, 10) + 0x0100_0000_0000u64,
            BigNum::new_raw(0xFFFF_FFFF_FFFF_FFFF, 10)
        );
        assert_eq_bignum!(
            BigNum::new(0xFFFF_FFFF_FFFF_FFFFu64, 0xFFFF_FFFF_FFFF_0000) + 0x0100_0000_0000u64,
            BigNum::new_raw(0xFFFF_FFFF_FFFF_FFFF, 0xFFFF_FFFF_FFFF_0000)
        );
        assert_eq_bignum!(
            BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 0xFFFF_FFFF)
                + BigNum::new(0x1FFF_FFFF_FFFF_FFFF, 0xFFFF_FFF0),
            BigNum::new_raw(0x1000_0000_0000_0000, 0x1_0000_0000)
        );
        assert_eq_bignum!(
            BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 0xFFFF_FFFF)
                + BigNum::new(0x1FFF_FFFF_FFFF_FFFF, 0xFFFF_FFEF),
            BigNum::new_raw(0xFFFF_FFFF_FFFF_FFFF, 0xFFFF_FFFF)
        );
    }

    #[test]
    fn add_decimal_test() {
        type BigNum = BigNumBase<Decimal>;

        assert_eq_bignum!(
            BigNum::from(1) + BigNum::new(1243123123, 3),
            BigNum::new_raw(1243123123001, 0)
        );
        assert_eq_bignum!(
            BigNum::from(1000) + BigNum::new(10u64.pow(19) - 1, 3),
            BigNum::new_raw(10u64.pow(18), 4)
        );
        assert_eq_bignum!(
            BigNum::new(10u64.pow(19) - 1, 13) + BigNum::new(10u64.pow(18), 3),
            BigNum::new_raw(10u64.pow(18) + 10u64.pow(7) - 1, 14)
        );
    }

    #[test]
    fn add_arbitrary_test() {
        create_default_base!(Base61, 61);
        type BigNum = BigNumBase<Base61>;

        let SigRange(min_sig, max_sig) = Base61::calculate_ranges().1;

        assert_eq_bignum!(
            BigNum::from(0xFFFF_FFFF_FFFF_FFFEu64) + 1u64,
            BigNum::new_raw(((u64::MAX as u128 + 1) / 61u128) as u64, 1)
        );
        assert_eq_bignum!(BigNum::from(1u64) + 1u64, BigNum::new_raw(2, 0));
        assert_eq_bignum!(
            //BigNum::new(max_sig, 10, BASE) + BigNum::new(1, 10, BASE),
            BigNum::new(max_sig, 10) + BigNum::new(1, 10),
            BigNum::new_raw(min_sig, 11)
        );
        assert_eq_bignum!(
            BigNum::new(max_sig, 10) + BigNum::new(61u64, 9),
            BigNum::new_raw(min_sig, 11)
        );
    }

    #[test]
    fn sub_binary_test() {
        type BigNum = BigNumBase<Binary>;

        assert_eq_bignum!(
            BigNum::new(0x100, 32) - BigNum::new(0x0080_0000_0000, 0),
            BigNum::new_raw(0x0080_0000_0000, 0)
        );
        assert_eq_bignum!(
            BigNum::new(0x1000_0000_0000_0000, 0) - BigNum::new(0x0010_0000_0000_0000, 8),
            BigNum::from(0)
        );
        assert_eq_bignum!(
            BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 48) - BigNum::new(0x8000_0000_0000_0000, 16),
            BigNum::new(0xFFFF_FFFF_7FFF_FFFF, 48)
        );
        assert_eq_bignum!(
            BigNum::new(0xFFFF_FFFF_FFFF_FFFF, 48) - BigNum::new(0xFFFF_FFFF_0000_0000, 48),
            BigNum::new(0xFFFF_FFFF_0000_0000, 16)
        );
        assert_eq_bignum!(
            BigNum::new(0x8000_0000_0000_0000, 48) - BigNum::new(0xFFFF_FFFF_0000_0000, 16),
            BigNum::new(0xFFFF_FFFE_0000_0002, 47)
        );
    }

    // I won't test each individual base since the logic is the same, but I will test
    // binary and arbitrary
    #[test]
    fn sub_arbitrary_test() {
        create_default_base!(Base61, 61);
        type BigNum = BigNumBase<Base61>;

        let SigRange(min_sig, max_sig) = Base61::calculate_ranges().1;

        // This is an example of how subtraction results in a loss of precision. I may
        // do a lossless_sub trait at some point that casts both sigs to u128 before
        // calculating

        assert_eq_bignum!(
            BigNum::new(min_sig, 1) - 61u64,
            BigNum::new_raw(max_sig - 60, 0)
        );
        assert_eq_bignum!(
            BigNum::new(max_sig, 1) - max_sig,
            BigNum::new_raw(max_sig - max_sig / 61, 1)
        );
        assert_eq_bignum!(
            BigNum::new(12341098709128730491, 11234) - BigNum::new(12341098709128730491, 11234),
            BigNum::from(0)
        )
    }

    #[test]
    fn mul_binary_test() {
        type BigNum = BigNumBase<Binary>;
        let SigRange(min_sig, max_sig) = Binary::calculate_ranges().1;

        assert_eq_bignum!(
            BigNum::from(14215125) * BigNum::from(120487091724u64),
            BigNum::from(120487091724u64 * 14215125)
        );
        // 2^63 * 2^63 = 2^126
        assert_eq_bignum!(
            BigNum::from(min_sig) * BigNum::from(min_sig),
            BigNum::new(min_sig, 63)
        );
        assert_eq_bignum!(
            BigNum::from(min_sig) * BigNum::from(min_sig),
            BigNum::new(min_sig, 63)
        );
        assert_eq_bignum!(
            BigNum::new(max_sig, 1) * BigNum::new(max_sig, 1),
            BigNum::new(max_sig - 1, 64 + 2)
        );
        assert_eq_bignum!(
            BigNum::new(max_sig, 1123) * BigNum::new(max_sig, 11325),
            BigNum::new(max_sig - 1, 64 + 1123 + 11325)
        );
        assert_eq_bignum!(
            BigNum::new(max_sig - min_sig, 123410923) * BigNum::from(0),
            BigNum::from(0)
        );
        assert_eq_bignum!(
            BigNum::new(max_sig - min_sig, 123410923) * BigNum::from(1),
            BigNum::new(max_sig - min_sig, 123410923)
        );
    }

    #[test]
    fn binary_div_test() {
        type BigNum = BigNumBase<Binary>;
        let SigRange(min_sig, max_sig) = Binary::calculate_ranges().1;

        assert_eq_bignum!(
            BigNum::from(123412341234432u64) / BigNum::from(1221314),
            BigNum::from(123412341234432u64 / 1221314)
        );
        assert_eq_bignum!(
            BigNum::from(123412341234432u64) / BigNum::from(123412341234432u64),
            BigNum::from(1)
        );
        assert_eq_bignum!(
            BigNum::from(123412341234432u64) / BigNum::from(12341234123412341234u64),
            BigNum::from(0)
        );
        assert_eq_bignum!(
            BigNum::new(123412341234432u64, 12341234) / BigNum::new(123412341234432u64, 12341234),
            BigNum::from(1)
        );
        assert_eq_bignum!(
            BigNum::new(123412341234432u64, 12341234) / BigNum::new(123412341234432u64, 12341235),
            BigNum::from(0)
        );
        assert_eq_bignum!(
            BigNum::new(123412341234432u64, 12341234) / BigNum::new(123412341234433u64, 12341234),
            BigNum::from(0)
        );
        assert_eq_bignum!(
            BigNum::new(min_sig, 12341234) / BigNum::new(min_sig, 12341233),
            BigNum::from(2)
        );
        assert_eq_bignum!(
            BigNum::new(min_sig, 12341234) / BigNum::new(min_sig, 1),
            BigNum::new(min_sig, 12341234 - 64)
        );
        assert_eq_bignum!(
            BigNum::new(max_sig, 12341234) / BigNum::new(max_sig, 1),
            BigNum::new(min_sig, 12341234 - 64)
        );
        assert_eq_bignum!(
            BigNum::new(max_sig, 12341234) / BigNum::new(min_sig, 1),
            BigNum::new(max_sig, 12341234 - 64)
        );
        assert_eq_bignum!(
            BigNum::new(max_sig, 63 + 12341234) / BigNum::new(min_sig, 1),
            BigNum::new(max_sig, 12341234 - 1)
        );
    }

    #[test]
    fn binary_shifts() {
        type BigNum = BigNumBase<Binary>;

        assert_eq_bignum!(BigNum::new(0b100, 0) << 1, BigNum::new(0b1000, 0));
        assert_eq_bignum!(BigNum::new(0b100, 0) << 2, BigNum::new(0b10000, 0));
        assert_eq_bignum!(BigNum::new(u64::MAX, 1) << 3, BigNum::new(u64::MAX, 4));
        assert_eq_bignum!(BigNum::new(u64::MAX, 0) << 64, BigNum::new(u64::MAX, 64));

        assert_eq_bignum!(BigNum::new(0b100, 0) >> 1, BigNum::new(0b10, 0));
        assert_eq_bignum!(BigNum::new(0b100, 0) >> 2, BigNum::new(0b1, 0));
        assert_eq_bignum!(BigNum::new(u64::MAX, 1) >> 3, BigNum::new(u64::MAX / 4, 0));
        assert_eq_bignum!(BigNum::new(u64::MAX, 0) >> 63, BigNum::from(1));
        assert_eq_bignum!(
            BigNum::new(u64::MAX, 100) >> 105,
            BigNum::new(u64::MAX / 32, 0)
        );
    }

    #[test]
    fn display_test() {
        type BigNum = BigNumBase<Decimal>;

        assert_eq!(format!("{}", BigNum::from(1)), "1");
        assert_eq!(format!("{}", BigNum::from(999)), "999");
        assert_eq!(format!("{}", BigNum::from(1000)), "1k");
        assert_eq!(format!("{}", BigNum::from(1001)), "1.001k");
        assert_eq!(format!("{}", BigNum::from(999999)), "999.9k");
        assert_eq!(format!("{}", BigNum::from(1000000)), "1m");
        assert_eq!(format!("{}", BigNum::from(1001000)), "1.001m");
        assert_eq!(format!("{}", BigNum::from(999999999)), "999.9m");
        assert_eq!(format!("{}", BigNum::from(1000000000)), "1b");
        assert_eq!(format!("{}", BigNum::from(1001000000)), "1.001b");
        assert_eq!(format!("{}", BigNum::from(999999999999)), "999.9b");
        assert_eq!(format!("{}", BigNum::from(1000000000000)), "1t");
        assert_eq!(format!("{}", BigNum::from(1001000000000)), "1.001t");
        assert_eq!(format!("{}", BigNum::from(999999999999999)), "999.9t");
        assert_eq!(format!("{}", BigNum::from(1000000000000000)), "1e15");
        assert_eq!(format!("{}", BigNum::from(1001000000000000)), "1.001e15");
        assert_eq!(format!("{}", BigNum::from(999999999999999999)), "9.999e17");
        assert_eq!(format!("{}", BigNum::new(9999, 123523)), "9.999e123526");
        assert_eq!(format!("{}", BigNum::new(9099, 123523)), "9.099e123526");
        assert_eq!(format!("{}", BigNum::new(999, 123523)), "9.99e123525");
    }

    #[test]
    fn test_random_bin() {
        #![allow(clippy::erasing_op)]

        type BigNum = BigNumBase<Binary>;

        let dist: Uniform<BigNum> = Uniform::new(BigNum::from(0), BigNum::new(1, u64::MAX / 2));
        let rng = &mut thread_rng();
        let nums = dist.sample_iter(rng).take(100);

        for n in nums {
            assert_eq_bignum!(n + n, n * 2);
            assert_eq_bignum!(n + n + n, n * BigNum::new(3, 0));
            assert_eq_bignum!(2 * n + 2 * n, 4 * n);
            assert_eq_bignum!(n / 2 / 16, n / 32);
            assert_eq_bignum!(n * 0, n / (n.succ()));
            assert_eq_bignum!(n + 0, n / 1);
            assert_eq_bignum!(n / n, BigNum::from(1));
        }
    }

    #[test]
    fn sum_test_binary() {
        type BigNum = BigNumBin;

        let a: [BigNum; 0] = [];
        let b: [BigNum; 10] = [BigNum::from(100); 10];
        let c = (0u64..100).map(BigNum::from);
        let d: [BigNum; 100] = [BigNum::from(1 << 63); 100];

        assert_eq!(BigNum::from(0), a.into_iter().sum());
        assert_eq!(BigNum::from(1000), b.into_iter().sum());
        assert_eq!(BigNum::from(4950), c.sum());

        assert_eq!(BigNum::from(1 << 63) * 100, d.into_iter().sum());
    }

    #[test]
    fn prod_test_binary() {
        type BigNum = BigNumBin;

        let a: [BigNum; 0] = [];
        let b: [BigNum; 10] = [BigNum::from(2); 10];
        let c: [BigNum; 10] = [BigNum::from(8); 10];
        let d: [BigNum; 100] = [BigNum::from(1 << 63); 100];

        assert_eq!(BigNum::from(0), a.into_iter().product());
        assert_eq!(BigNum::from(1024), b.into_iter().product());
        assert_eq!(BigNum::from(1024 * 1024 * 1024), c.into_iter().product());

        assert_eq!(BigNum::new(1, 63 * 100), d.into_iter().product());
    }

    #[should_panic]
    #[test]
    fn fuzzy_eq_failed1() {
        type BigNum = BigNumDec;

        let a = BigNum::new(DEC_SIG_RANGE.1, 234);
        let b = a + a + a + a + a;
        let c = 2 * a + 3 * a;

        assert_eq!(b, c);
    }

    #[should_panic]
    #[test]
    fn fuzzy_eq_failed2() {
        type BigNum = BigNumDec;

        let a = BigNum::new(DEC_SIG_RANGE.1, 234);
        let d: BigNum = repeat_n(a, 20).sum();
        let e = a * 20;

        assert_eq!(d, e)
    }

    #[test]
    fn fuzzy_eq_test() {
        type BigNum = BigNumDec;

        let a = BigNum::new(DEC_SIG_RANGE.1, 234);
        let b = a + a + a + a + a;
        let c = 2 * a + 3 * a;
        let d: BigNum = repeat_n(a, 20).sum();
        let e = a * 20;

        // Since we apply 4 operations to b this is a good upper bound
        assert!(b.fuzzy_eq(c, 4));
        // Since we apply 20 operations to d this is a good upper bound
        assert!(d.fuzzy_eq(e, 20));
    }

    #[test]
    fn float_mult_test() {
        type BigNum = BigNumDec;

        let a = BigNum::new(DEC_SIG_RANGE.0, 1234);
        let b = BigNum::new(DEC_SIG_RANGE.1, 1234);

        assert_eq!(a * 1.5, a * 3 / 2);
        assert_eq!(a * 12.5, a * 100 / 8);
        assert_eq!(a * 0.5, a / 2);

        // Result of adding a to a float that doesn't fit in u64 bounds
        let a_overflow_res = a * 1e250;
        let a_exp_res = a * BigNum::new(1, 250);
        let (min, max) = if a_overflow_res > a_exp_res {
            (a_exp_res, a_overflow_res)
        } else {
            (a_overflow_res, a_exp_res)
        };

        // Error in result is less than 1/100000 = .001%
        assert!(max / (max - min) > BigNum::from(100000));

        assert_eq!(b * 1.5, b * 3 / 2);
        assert_eq!(b * 12.5, b * 100 / 8);
        assert_eq!(b * 0.5, b / 2);

        let b_overflow_res = b * 1.234e280;
        let b_exp_res = b * BigNum::new(1234, 277);
        let (min, max) = if b_overflow_res > b_exp_res {
            (b_exp_res, b_overflow_res)
        } else {
            (b_overflow_res, b_exp_res)
        };

        // Error in result is less than 1/100000 = .001%
        assert!(max / (max - min) > BigNum::from(100000));
    }
}
