use crate::hash::{
    ExtendableHashTraitType, HashFunction, HashFunctionFactory, HashableSequence,
    UnextendableHashTraitType,
};
use crate::types::MinimizerType;
use std::mem::size_of;

pub struct ForwardSeqHashIterator<N: HashableSequence> {
    seq: N,
    mask: HashIntegerType,
    fh: HashIntegerType,
    k_minus1: usize,
}

#[inline(always)]
fn get_mask(k: usize) -> HashIntegerType {
    HashIntegerType::MAX >> ((((size_of::<HashIntegerType>() * 4) - k) * 2) as HashIntegerType)
}

impl<N: HashableSequence> ForwardSeqHashIterator<N> {
    pub fn new(seq: N, k: usize) -> Result<ForwardSeqHashIterator<N>, &'static str> {
        if k > seq.bases_count() || k > (size_of::<HashIntegerType>() * 4) {
            return Err("K out of range!");
        }

        let mut fh = 0;
        for i in 0..(k - 1) {
            fh = (fh << 2) | unsafe { seq.get_unchecked_cbase(i) as HashIntegerType };
        }

        let mask = get_mask(k);

        Ok(ForwardSeqHashIterator {
            seq,
            mask,
            fh: fh & mask,
            k_minus1: k - 1,
        })
    }

    #[inline(always)]
    fn roll_hash(&mut self, index: usize) -> ExtForwardSeqHash {
        assert!(unsafe { self.seq.get_unchecked_cbase(index) } < 4);

        self.fh = ((self.fh << 2)
            | unsafe { self.seq.get_unchecked_cbase(index) as HashIntegerType })
            & self.mask;
        ExtForwardSeqHash(self.fh)
    }
}

impl<N: HashableSequence> HashFunction<ForwardSeqHashFactory> for ForwardSeqHashIterator<N> {
    type IteratorType =
        impl Iterator<Item = <ForwardSeqHashFactory as HashFunctionFactory>::HashTypeExtendable>;
    type EnumerableIteratorType = impl Iterator<
        Item = (
            usize,
            <ForwardSeqHashFactory as HashFunctionFactory>::HashTypeExtendable,
        ),
    >;

    fn iter(mut self) -> Self::IteratorType {
        (self.k_minus1..self.seq.bases_count()).map(move |idx| self.roll_hash(idx))
    }

    fn iter_enumerate(mut self) -> Self::EnumerableIteratorType {
        (self.k_minus1..self.seq.bases_count())
            .map(move |idx| (idx - self.k_minus1, self.roll_hash(idx)))
    }
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct ForwardSeqHashFactory;

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct ExtForwardSeqHash(HashIntegerType);

impl ExtendableHashTraitType for ExtForwardSeqHash {
    type HashTypeUnextendable = HashIntegerType;

    #[inline(always)]
    fn to_unextendable(self) -> Self::HashTypeUnextendable {
        self.0
    }
}

impl HashFunctionFactory for ForwardSeqHashFactory {
    type HashTypeUnextendable = HashIntegerType;
    type HashTypeExtendable = ExtForwardSeqHash;
    type HashIterator<N: HashableSequence> = ForwardSeqHashIterator<N>;
    const NULL_BASE: u8 = 0;

    fn new<N: HashableSequence>(seq: N, k: usize) -> Self::HashIterator<N> {
        ForwardSeqHashIterator::new(seq, k).unwrap()
    }

    fn get_bucket(hash: Self::HashTypeUnextendable) -> u32 {
        let mut x = hash;
        // x ^= x >> 12; // a
        // x ^= x << 25; // b
        // x ^= x >> 27; // c
        x as u32
    }

    fn get_second_bucket(hash: Self::HashTypeUnextendable) -> u32 {
        panic!("Not supported!")
    }

    fn get_minimizer(hash: Self::HashTypeUnextendable) -> MinimizerType {
        panic!("Not supported!")
    }

    fn get_shifted(hash: Self::HashTypeUnextendable, shift: u8) -> u8 {
        (hash >> shift) as u8
    }

    fn manual_roll_forward(
        hash: Self::HashTypeExtendable,
        k: usize,
        _out_base: u8,
        in_base: u8,
    ) -> Self::HashTypeExtendable {
        assert!(in_base < 4);
        // K = 2
        // 00AABB => roll CC
        // 00BBCC

        let mask = get_mask(k);
        ExtForwardSeqHash(((hash.0 << 2) | (in_base as HashIntegerType)) & mask)
    }

    fn manual_roll_reverse(
        hash: Self::HashTypeExtendable,
        k: usize,
        _out_base: u8,
        in_base: u8,
    ) -> Self::HashTypeExtendable {
        assert!(in_base < 4);
        // K = 2
        // 00AABB => roll rev CC
        // 00CCAA

        ExtForwardSeqHash((hash.0 >> 2) | ((in_base as HashIntegerType) << ((k - 1) * 2)))
    }

    fn manual_remove_only_forward(
        hash: Self::HashTypeExtendable,
        k: usize,
        _out_base: u8,
    ) -> Self::HashTypeExtendable {
        // K = 2
        // 00AABB => roll
        // 0000BB
        let mask = get_mask(k - 1);
        ExtForwardSeqHash(hash.0 & mask)
    }

    fn manual_remove_only_reverse(
        hash: Self::HashTypeExtendable,
        k: usize,
        _out_base: u8,
    ) -> Self::HashTypeExtendable {
        // K = 2
        // 00AABB => roll rev
        // 0000AA
        ExtForwardSeqHash(hash.0 >> 2)
    }
}

#[cfg(test)]
mod tests {

    use super::ForwardSeqHashFactory;
    use super::HashIntegerType;
    use crate::hash::tests::test_hash_function;
    use std::mem::size_of;

    #[test]
    fn fw_seqhash_test() {
        test_hash_function::<ForwardSeqHashFactory>(
            &(2..(size_of::<HashIntegerType>() * 4)).collect::<Vec<_>>(),
            false,
        );
    }
}
