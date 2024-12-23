//! This module contains additional traits I thought may be useful for BigNum usage.

use crate::{Base, BigNumBase, SigRange};

/// This trait gets the very next valid value of a type. Mainly for `BigNum`, since adding
/// one often doesn't result in a changing value. This is provided for contexts where you
/// need to increase the value easily
pub trait Succ {
    fn succ(self) -> Self;
}

/// This trait gets the previous valid value of a type. Mainly for `BigNum`, since subbing
/// one often doesn't result in a changing value. This is provided for contexts where you
/// need to decrease the value easily
pub trait Pred {
    fn pred(self) -> Self;
}

impl<T> Succ for BigNumBase<T>
where
    T: Base,
{
    fn succ(self) -> Self {
        let SigRange(min_sig, max_sig) = self.base.sig_range();

        if self.sig == max_sig {
            Self {
                sig: min_sig,
                exp: self.exp + 1,
                base: self.base,
            }
        } else {
            Self {
                sig: self.sig + 1,
                ..self
            }
        }
    }
}

impl<T> Pred for BigNumBase<T>
where
    T: Base,
{
    fn pred(self) -> Self {
        let SigRange(min_sig, max_sig) = self.base.sig_range();

        if self.exp == 0 {
            if self.sig == 0 {
                panic!("Cannot get the predecessor of 0");
            }

            Self {
                sig: self.sig - 1,
                ..self
            }
        } else if self.sig == min_sig {
            Self {
                sig: max_sig,
                exp: self.exp - 1,
                base: self.base,
            }
        } else {
            Self {
                sig: self.sig - 1,
                ..self
            }
        }
    }
}

pub trait BigNumPow<T>
where
    T: Base,
{
    fn pow(self, n: i32) -> BigNumBase<T>;
}

impl<T> BigNumPow<T> for f64
where
    T: Base,
{
    fn pow(self, n: i32) -> BigNumBase<T> {
        let mut res: BigNumBase<T> = 1u64.into();

        let max_pow = f64::MAX.log(T::NUMBER as f64).floor() as i32 - 1;

        if n <= max_pow {
            res *= self.powi(n);
        } else {
            let mut remaining_pow = n;
            let mut divisions = 0;

            loop {
                if remaining_pow <= max_pow {
                    //res *= self.powi(remaining_pow);
                    res *= self.powi(remaining_pow);
                    for _ in 0..divisions {
                        res *= res;
                    }
                    break;
                } else if remaining_pow <= max_pow * 25 {
                    remaining_pow -= max_pow;
                    res *= self.powi(max_pow);
                } else {
                    remaining_pow /= 2;
                    divisions += 1;
                }
            }
        }

        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        create_default_base, macros::test_macros::assert_eq_bignum, BigNumBase, BigNumBin, Binary,
    };

    // Other tests are in the normal macro so we can test it with many different bases
    #[test]
    fn test_succ() {
        type BigNum = BigNumBase<Binary>;
        let SigRange(min_sig, max_sig) = Binary::calculate_ranges().1;

        assert_eq_bignum!(BigNum::new(0, 0).succ(), BigNum::new(1, 0));
        assert_eq_bignum!(BigNum::new(max_sig, 0).succ(), BigNum::new(min_sig, 1));
        assert_eq_bignum!(BigNum::new(max_sig, 1).succ(), BigNum::new(min_sig, 2));
    }

    #[test]
    fn test_pred() {
        type BigNum = BigNumBase<Binary>;
        let SigRange(min_sig, max_sig) = Binary::calculate_ranges().1;

        assert_eq_bignum!(BigNum::new(1, 0).pred(), BigNum::new(0, 0));
        assert_eq_bignum!(BigNum::new(min_sig, 1).pred(), BigNum::new(max_sig, 0));
        assert_eq_bignum!(BigNum::new(min_sig, 2).pred(), BigNum::new(max_sig, 1));
    }

    #[test]
    fn test_bignum_pow() {
        type BigNum = BigNumBase<Binary>;
        create_default_base!(Base3, 3);
        type BigNum3 = BigNumBase<Base3>;
        create_default_base!(Base1422, 1422);
        type BigNum1422 = BigNumBase<Base1422>;

        let res: BigNum = 2.0.pow(1000);
        let res3: BigNum3 = 3.0.pow(1000);
        let res1422: BigNum1422 = 1422.0.pow(1000);

        assert!(res.fuzzy_eq(BigNum::new(1, 1000), 1000));
        assert!(res3.fuzzy_eq(BigNum3::new(1, 1000), 1000000));
        assert!(
            res1422.fuzzy_eq(BigNum1422::new(1, 1000), 1000000),
            "{:?} {:?}",
            res1422,
            BigNum1422::new(1, 1000)
        );
    }

    #[test]
    fn test_bignum_pow2() {
        type BigNum = BigNumBin;
        assert_eq!(2f64.pow(10), BigNum::from(1024));
        assert_eq!(2f64.pow(20), BigNum::from(1024 * 1024));

        let act: BigNum = 2f64.pow(200_000);
        let exp = BigNum::new(1, 200_000);
        let (min, max) = if act > exp { (exp, act) } else { (act, exp) };

        assert!(
            min.fuzzy_eq(max, u64::MAX / 1_000_000),
            "{:?} {:?}",
            min,
            max
        );

        let act: BigNum = 2f64.pow(2_000_000);
        let exp = BigNum::new(1, 2_000_000);
        let (min, max) = if act > exp { (exp, act) } else { (act, exp) };

        assert!(
            min.fuzzy_eq(max, u64::MAX / 1_000_000),
            "{:?} {:?}",
            min,
            max
        );

        let act: BigNum = 2f64.pow(2_000_000_000);
        let exp = BigNum::new(1, 2_000_000_000);
        let (min, max) = if act > exp { (exp, act) } else { (act, exp) };

        // With powers this large we give up a huge amount of precision so it's not recommended
        assert!(
            max.exp as f64 / (max.exp - min.exp) as f64 > 10_000.,
            "{} {}",
            min.exp,
            max.exp
        );
    }
}
