pub(crate) const SIMD_WIDTH: usize = 8;

// Upstream gates a `fearless_simd` implementation of this module behind a
// `simd` feature and ships the scalar version below for when it is off.
// This fork carries neither the dependency nor the feature, so only the
// scalar one can ever compile -- and a `cfg` on a feature that does not
// exist is a warning, which `clippy -D warnings` makes a gate failure.
//
// Recorded in docs/UPSTREAM_FORKS.toml rather than left as a silent
// difference: taking it back means adding fearless_simd, which is a new
// external dependency and a decision of its own.
// Lane-wise loops over a fixed-width array, indexed on purpose: each one is the
// scalar spelling of one SIMD instruction, and `i` is the lane. Rewriting them
// as iterator chains hides that correspondence, which is the only thing making
// this module reviewable against the vector implementation it stands in for.
// Upstream allows the same lint across its workspace; this fork does not
// inherit that, so it is said here with its reason.
#[allow(clippy::needless_range_loop)]
mod inner {
    use super::SIMD_WIDTH;
    use core::marker::PhantomData;
    use core::ops::{BitAnd, BitOr, BitXor, BitXorAssign};

    pub(crate) trait Simd: Copy + Clone {}

    #[derive(Copy, Clone)]
    pub(crate) struct ScalarSimd;
    impl Simd for ScalarSimd {}

    pub(crate) struct Level;
    impl Level {
        #[inline(always)]
        pub(crate) fn new() -> Self {
            Level
        }
    }

    macro_rules! simd_dispatch {
        ($level:expr, $simd:ident => $body:expr) => {{
            let _ = $level;
            let $simd = $crate::simd::ScalarSimd;
            $body
        }};
    }

    pub(crate) use simd_dispatch as dispatch;

    #[derive(Copy, Clone)]
    #[allow(non_camel_case_types)]
    #[repr(C, align(32))]
    pub(crate) struct mask32x8<S: Simd> {
        val: [bool; SIMD_WIDTH],
        _marker: PhantomData<S>,
    }

    impl<S: Simd> mask32x8<S> {
        #[inline(always)]
        pub(crate) fn select(self, if_true: u32x8<S>, if_false: u32x8<S>) -> u32x8<S> {
            let mut result = [0u32; SIMD_WIDTH];
            for i in 0..SIMD_WIDTH {
                result[i] = if self.val[i] {
                    if_true.val[i]
                } else {
                    if_false.val[i]
                };
            }
            u32x8 {
                val: result,
                _marker: PhantomData,
            }
        }
    }

    #[derive(Copy, Clone)]
    #[allow(non_camel_case_types)]
    #[repr(C, align(32))]
    pub(crate) struct u32x8<S: Simd> {
        pub(super) val: [u32; SIMD_WIDTH],
        _marker: PhantomData<S>,
    }

    impl<S: Simd> u32x8<S> {
        #[inline(always)]
        pub(crate) fn from_slice(_simd: S, slice: &[u32]) -> Self {
            let mut val = [0u32; SIMD_WIDTH];
            val.copy_from_slice(&slice[..SIMD_WIDTH]);
            Self {
                val,
                _marker: PhantomData,
            }
        }

        #[inline(always)]
        pub(crate) fn splat(_simd: S, val: u32) -> Self {
            Self {
                val: [val; SIMD_WIDTH],
                _marker: PhantomData,
            }
        }

        #[inline(always)]
        pub(crate) fn store(self, slice: &mut [u32]) {
            slice[..SIMD_WIDTH].copy_from_slice(&self.val);
        }

        #[inline(always)]
        pub(crate) fn simd_gt(self, other: Self) -> mask32x8<S> {
            let mut result = [false; SIMD_WIDTH];
            for i in 0..SIMD_WIDTH {
                result[i] = self.val[i] > other.val[i];
            }
            mask32x8 {
                val: result,
                _marker: PhantomData,
            }
        }
    }

    impl<S: Simd> BitAnd for u32x8<S> {
        type Output = Self;
        #[inline(always)]
        fn bitand(self, rhs: Self) -> Self {
            let mut result = [0u32; SIMD_WIDTH];
            for i in 0..SIMD_WIDTH {
                result[i] = self.val[i] & rhs.val[i];
            }
            Self {
                val: result,
                _marker: PhantomData,
            }
        }
    }

    impl<S: Simd> BitOr for u32x8<S> {
        type Output = Self;
        #[inline(always)]
        fn bitor(self, rhs: Self) -> Self {
            let mut result = [0u32; SIMD_WIDTH];
            for i in 0..SIMD_WIDTH {
                result[i] = self.val[i] | rhs.val[i];
            }
            Self {
                val: result,
                _marker: PhantomData,
            }
        }
    }

    impl<S: Simd> BitXor for u32x8<S> {
        type Output = Self;
        #[inline(always)]
        fn bitxor(self, rhs: Self) -> Self {
            let mut result = [0u32; SIMD_WIDTH];
            for i in 0..SIMD_WIDTH {
                result[i] = self.val[i] ^ rhs.val[i];
            }
            Self {
                val: result,
                _marker: PhantomData,
            }
        }
    }

    impl<S: Simd> BitXorAssign for u32x8<S> {
        #[inline(always)]
        fn bitxor_assign(&mut self, rhs: Self) {
            for i in 0..SIMD_WIDTH {
                self.val[i] ^= rhs.val[i];
            }
        }
    }
}

pub(crate) use inner::*;
