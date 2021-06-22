use crate::intermediate_storage::{IntermediateReadsWriter, IntermediateSequencesStorage};
use crate::pipeline::links_compaction::LinkMapping;
use crate::pipeline::Pipeline;
use crate::reads_freezer::{FastaWriterConcurrentBuffer, ReadsFreezer};
use crate::rolling_minqueue::RollingMinQueue;
use crate::sequences_reader::{FastaSequence, SequencesReader};
use crate::unitig_link::UnitigIndex;
use crate::utils::Utils;
use crossbeam::channel::*;
use crossbeam::queue::{ArrayQueue, SegQueue};
use crossbeam::{scope, thread};
use nix::sys::ptrace::cont;
use object_pool::Pool;
use parallel_processor::multi_thread_buckets::MultiThreadBuckets;
use parallel_processor::smart_bucket_sort::{smart_radix_sort, SortKey};
use rayon::iter::ParallelIterator;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator};
use std::fs::File;
use std::intrinsics::unlikely;
use std::io::Cursor;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{sleep, Thread};
use std::time::{Duration, Instant};

impl Pipeline {
    pub fn reorganize_reads(
        mut reads: Vec<PathBuf>,
        mut mapping_files: Vec<PathBuf>,
        output_path: &Path,
        buckets_count: usize,
        k: usize,
        m: usize,
    ) -> Vec<PathBuf> {
        let mut buckets = MultiThreadBuckets::<IntermediateReadsWriter<UnitigIndex>>::new(
            buckets_count,
            &output_path.join("reads_bucket"),
            None,
        );

        let mut final_unitigs_file = Mutex::new(ReadsFreezer::optfile_splitted_compressed_lz4(
            format!("{}", output_path.join("output.fa").display()),
        ));

        reads.sort();
        mapping_files.sort();

        let inputs: Vec<_> = reads.iter().zip(mapping_files.iter()).collect();

        inputs.par_iter().for_each(|(read_file, mapping_file)| {
            let mut tmp_reads_buffer = IntermediateSequencesStorage::new(buckets_count, &buckets);
            let mut tmp_lonely_unitigs_buffer =
                FastaWriterConcurrentBuffer::new(&final_unitigs_file, 1024 * 1024 * 8);

            let mut mappings = Vec::new();

            assert_eq!(
                Utils::get_bucket_index(read_file),
                Utils::get_bucket_index(mapping_file)
            );

            let bucket_index = Utils::get_bucket_index(read_file);

            let mappings_file = filebuffer::FileBuffer::open(mapping_file).unwrap();
            let mut reader = Cursor::new(mappings_file.deref());
            while let Some(link) = LinkMapping::from_stream(&mut reader) {
                mappings.push(link);
            }

            struct Compare {}
            impl SortKey<LinkMapping> for Compare {
                type KeyType = u64;
                const KEY_BITS: usize = 64;

                fn get(value: &LinkMapping) -> u64 {
                    value.entry
                }

                fn get_shifted(value: &LinkMapping, rhs: u8) -> u8 {
                    (value.entry >> rhs) as u8
                }
            }

            smart_radix_sort::<_, Compare, false>(&mut mappings[..]);

            let mut index = 0;
            let mut map_index = 0;

            SequencesReader::process_file_extended(read_file, |seq| {
                if map_index < mappings.len() && mappings[map_index].entry == index {
                    // Mapping found
                    tmp_reads_buffer.add_read(
                        UnitigIndex::new(bucket_index, index as usize, false),
                        seq.seq,
                        mappings[map_index].bucket,
                    );
                    map_index += 1;
                } else {
                    tmp_lonely_unitigs_buffer.add_read(FastaSequence {
                        ident: format!("{} {}", bucket_index, index).as_bytes(),
                        seq: seq.seq,
                        qual: None,
                    });
                    // No mapping, write unitig to file
                }
                index += 1;
            });
            tmp_lonely_unitigs_buffer.finalize();
            if mappings.len() > 0 {
                println!("Total reads: {}/{:?}", index, mappings.last().unwrap());
            }
            assert_eq!(map_index, mappings.len())
        });

        final_unitigs_file.into_inner().unwrap().finalize();

        buckets.finalize()
    }
}
