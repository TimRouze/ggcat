use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::ops::{Shl, Shr};

use crate::hashes::fw_nthash::ForwardNtHashIterator;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::types::{BucketIndexType, MinimizerType};

pub trait UnextendableHashTraitType =
    Copy + Clone + Debug + Display + Eq + Ord + Hash + Send + Sync + Serialize + DeserializeOwned;

pub trait ExtendableHashTraitType: Copy + Clone + Debug + Eq + Ord + Send + Sync {
    type HashTypeUnextendable: UnextendableHashTraitType;
    fn to_unextendable(self) -> Self::HashTypeUnextendable;
}

pub trait HashFunctionFactory: Ord + Sized + Clone + Debug {
    type HashTypeUnextendable: UnextendableHashTraitType;
    type HashTypeExtendable: ExtendableHashTraitType<
        HashTypeUnextendable = Self::HashTypeUnextendable,
    >;
    type HashIterator<N: HashableSequence>: HashFunction<Self>;

    const NULL_BASE: u8;

    fn new<N: HashableSequence>(seq: N, k: usize) -> Self::HashIterator<N>;

    fn get_bucket(hash: Self::HashTypeUnextendable) -> BucketIndexType;
    fn get_second_bucket(hash: Self::HashTypeUnextendable) -> BucketIndexType;
    fn get_minimizer(hash: Self::HashTypeUnextendable) -> MinimizerType;
    fn get_shifted(hash: Self::HashTypeUnextendable, shift: u8) -> u8;

    fn manual_roll_forward(
        hash: Self::HashTypeExtendable,
        k: usize,
        out_base: u8,
        in_base: u8,
    ) -> Self::HashTypeExtendable;

    fn manual_roll_reverse(
        hash: Self::HashTypeExtendable,
        k: usize,
        out_base: u8,
        in_base: u8,
    ) -> Self::HashTypeExtendable;

    fn manual_remove_only_forward(
        hash: Self::HashTypeExtendable,
        k: usize,
        out_base: u8,
    ) -> Self::HashTypeExtendable;

    fn manual_remove_only_reverse(
        hash: Self::HashTypeExtendable,
        k: usize,
        out_base: u8,
    ) -> Self::HashTypeExtendable;
}

pub trait HashFunction<HF: HashFunctionFactory> {
    type IteratorType: Iterator<Item = HF::HashTypeExtendable>;
    type EnumerableIteratorType: Iterator<Item = (usize, HF::HashTypeExtendable)>;

    fn iter(self) -> Self::IteratorType;
    fn iter_enumerate(self) -> Self::EnumerableIteratorType;
}

pub trait HashableSequence: Clone {
    unsafe fn get_unchecked_cbase(&self, index: usize) -> u8;
    fn bases_count(&self) -> usize;
}

impl HashableSequence for &[u8] {
    #[inline(always)]
    unsafe fn get_unchecked_cbase(&self, index: usize) -> u8 {
        *self.get_unchecked(index)
    }

    #[inline(always)]
    fn bases_count(&self) -> usize {
        self.len()
    }
}

#[cfg(test)]
pub mod tests {
    use crate::hash::ExtendableHashTraitType;
    use crate::hash::HashFunction;
    use crate::hash::HashFunctionFactory;
    use crate::utils::Utils;
    use rand::RngCore;

    // From rand test library
    /// Construct a deterministic RNG with the given seed
    pub fn rng(seed: u64) -> impl RngCore {
        // For tests, we want a statistically good, fast, reproducible RNG.
        // PCG32 will do fine, and will be easy to embed if we ever need to.
        const INC: u64 = 11634580027462260723;
        rand_pcg::Pcg32::new(seed, INC)
    }

    fn to_compressed(bases: &[u8]) -> Vec<u8> {
        let mut res = Vec::new();
        for base in bases {
            res.push(Utils::compress_base(*base))
        }
        res
    }

    fn compute_hashes<FACTORY: HashFunctionFactory>(
        bases: &[u8],
        k: usize,
        compress: bool,
    ) -> Vec<FACTORY::HashTypeExtendable> {
        let bases_vec;

        let bases = if compress {
            bases_vec = to_compressed(bases);
            bases_vec.as_slice()
        } else {
            bases
        };

        FACTORY::new(bases, k).iter().collect()
    }

    fn generate_bases(len: usize, seed: u64) -> Vec<u8> {
        let mut rng = rng(seed);

        let mut result = (0..len)
            .map(|x| Utils::decompress_base((rng.next_u32() % 4) as u8))
            .collect::<Vec<_>>();

        result
    }

    pub fn test_hash_function<FACTORY: HashFunctionFactory>(kvalues: &[usize], canonical: bool) {
        for kval in kvalues {
            let test_bases = generate_bases(*kval * 100, 182 + *kval as u64);

            let hashes = compute_hashes::<FACTORY>(test_bases.as_slice(), *kval, true);

            // Distribution test
            {
                let mut tmp = hashes
                    .iter()
                    .enumerate()
                    .map(|(i, x)| (*x, i))
                    .collect::<Vec<_>>();
                tmp.sort();
                for i in 1..tmp.len() {
                    if tmp[i - 1].0 == tmp[i].0 {
                        let f = tmp[i - 1].1;
                        let s = tmp[i].1;

                        let fx = &test_bases[f..f + *kval];
                        let sx = &test_bases[s..s + *kval];

                        if fx != sx {
                            panic!(
                                "Error collision {:?} {} != {}!",
                                tmp[i - 1].0,
                                std::str::from_utf8(fx).unwrap(),
                                std::str::from_utf8(sx).unwrap()
                            );
                        }
                    }
                }
            }

            // Double hash test
            {
                let mut dtest_bases = test_bases.clone();
                dtest_bases.extend_from_slice(test_bases.as_slice());

                let dhashes = compute_hashes::<FACTORY>(dtest_bases.as_slice(), *kval, true);

                let kmers_count = test_bases.len() - *kval + 1;

                let first_range = ..kmers_count;
                let second_range = (kmers_count + *kval - 1)..;

                assert_eq!(dhashes[first_range], dhashes[second_range]);
            }

            // Canonical test
            if canonical {
                let rc_bases = test_bases
                    .iter()
                    .map(|x| match *x {
                        b'A' => b'T',
                        b'C' => b'G',
                        b'G' => b'C',
                        b'T' => b'A',
                        _ => unreachable!(),
                    })
                    .rev()
                    .collect::<Vec<_>>();

                let rc_hashes = compute_hashes::<FACTORY>(rc_bases.as_slice(), *kval, true);

                assert_eq!(
                    hashes
                        .iter()
                        .map(|x| x.to_unextendable())
                        .collect::<Vec<_>>(),
                    rc_hashes
                        .iter()
                        .map(|x| x.to_unextendable())
                        .rev()
                        .collect::<Vec<_>>(),
                );
            }

            // Manual forward+reverse test
            {
                for i in 0..hashes.len() - 1 {
                    let manual_roll = FACTORY::manual_roll_forward(
                        hashes[i],
                        *kval,
                        Utils::compress_base(test_bases[i]),
                        Utils::compress_base(test_bases[i + *kval]),
                    );
                    assert_eq!(hashes[i + 1], manual_roll);
                }

                for i in (1..hashes.len()).rev() {
                    let manual_roll = FACTORY::manual_roll_reverse(
                        hashes[i],
                        *kval,
                        Utils::compress_base(test_bases[i + *kval - 1]),
                        Utils::compress_base(test_bases[i - 1]),
                    );
                    assert_eq!(hashes[i - 1], manual_roll);
                }
            }

            // Manual remove forward test
            {
                let lhashes = compute_hashes::<FACTORY>(test_bases.as_slice(), *kval - 1, true);

                for i in 0..hashes.len() {
                    let manual_roll = FACTORY::manual_remove_only_forward(
                        hashes[i],
                        *kval,
                        Utils::compress_base(test_bases[i]),
                    );
                    assert_eq!(lhashes[i + 1], manual_roll);
                }
            }

            // Manual remove backward test
            {
                let lhashes = compute_hashes::<FACTORY>(test_bases.as_slice(), *kval - 1, true);

                for i in (0..hashes.len()).rev() {
                    let manual_roll = FACTORY::manual_remove_only_reverse(
                        hashes[i],
                        *kval,
                        Utils::compress_base(test_bases[i + *kval - 1]),
                    );
                    assert_eq!(lhashes[i], manual_roll);
                }
            }
        }
    }
}
