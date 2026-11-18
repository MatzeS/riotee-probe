use std::collections::BTreeMap;

use ihex::{Reader, Record};

use crate::error::HexError;

/// A segment of contiguous 16-bit words starting at `address`.
pub struct Segment {
    pub address: u32,
    pub words: Vec<u16>,
}

/// Parses Intel HEX data into contiguous 16-bit word segments, merging
/// adjacent data records the way a flat memory image would be laid out.
pub fn parse_words(contents: &str) -> Result<Vec<Segment>, HexError> {
    // A byte map is not efficient, but unlikely to be critical here.
    let mut bytes: BTreeMap<u32, u8> = BTreeMap::new();
    let mut base: u32 = 0;

    for record in Reader::new(contents) {
        match record? {
            Record::Data { offset, value } => {
                let start = base + offset as u32;
                for (i, byte) in value.into_iter().enumerate() {
                    bytes.insert(start + i as u32, byte);
                }
            }
            Record::ExtendedLinearAddress(upper) => base = (upper as u32) << 16,
            Record::ExtendedSegmentAddress(segment) => base = (segment as u32) << 4,
            Record::EndOfFile => break,
            Record::StartSegmentAddress { .. } | Record::StartLinearAddress(_) => {}
        }
    }

    let mut segments = Vec::new();
    let mut iter = bytes.into_iter().peekable();
    while let Some((start, first_byte)) = iter.next() {
        let mut run = vec![first_byte];
        let mut next_addr = start + 1;
        while iter.peek().is_some_and(|&(addr, _)| addr == next_addr) {
            run.push(iter.next().unwrap().1);
            next_addr += 1;
        }
        if start % 2 != 0 {
            return Err(HexError::Unaligned { address: start });
        }
        if run.len() % 2 != 0 {
            return Err(HexError::OddLength {
                address: start,
                len: run.len(),
            });
        }
        let words = run
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&w| u16::from_le_bytes(w))
            .collect();
        segments.push(Segment {
            address: start,
            words,
        });
    }

    Ok(segments)
}

/// Splits a segment into packets of at most `max_words` words each, pairing
/// every chunk with the address it starts at.
pub fn packetize(segment: &Segment, max_words: usize) -> impl Iterator<Item = (u32, &[u16])> {
    let mut address = segment.address;
    segment.words.chunks(max_words).map(move |chunk| {
        let chunk_address = address;
        address += chunk.len() as u32 * 2;
        (chunk_address, chunk)
    })
}

#[cfg(test)]
mod tests {
    use ihex::create_object_file_representation;

    use super::*;

    #[test]
    fn merges_contiguous_data_into_one_segment() {
        let hex = create_object_file_representation(&[
            Record::Data {
                offset: 0x0000,
                value: vec![0x01, 0x02, 0x03, 0x04],
            },
            Record::Data {
                offset: 0x0004,
                value: vec![0x05, 0x06],
            },
            Record::EndOfFile,
        ])
        .unwrap();

        let segments = parse_words(&hex).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].address, 0);
        assert_eq!(segments[0].words, vec![0x0201, 0x0403, 0x0605]);
    }

    #[test]
    fn splits_non_contiguous_data_into_separate_segments() {
        let hex = create_object_file_representation(&[
            Record::Data {
                offset: 0x0000,
                value: vec![0x01, 0x02],
            },
            Record::Data {
                offset: 0x0010,
                value: vec![0x03, 0x04],
            },
            Record::EndOfFile,
        ])
        .unwrap();

        let segments = parse_words(&hex).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].address, 0x0000);
        assert_eq!(segments[1].address, 0x0010);
    }

    #[test]
    fn respects_extended_linear_address() {
        let hex = create_object_file_representation(&[
            Record::ExtendedLinearAddress(0x0001),
            Record::Data {
                offset: 0x0000,
                value: vec![0xAA, 0xBB],
            },
            Record::EndOfFile,
        ])
        .unwrap();

        let segments = parse_words(&hex).unwrap();
        assert_eq!(segments[0].address, 0x0001_0000);
    }

    #[test]
    fn rejects_odd_length_segments() {
        let hex = create_object_file_representation(&[
            Record::Data {
                offset: 0x0000,
                value: vec![0x01, 0x02, 0x03],
            },
            Record::EndOfFile,
        ])
        .unwrap();

        assert!(parse_words(&hex).is_err());
    }

    #[test]
    fn rejects_unaligned_segments() {
        let hex = create_object_file_representation(&[
            Record::Data {
                offset: 0x0001,
                value: vec![0x01, 0x02],
            },
            Record::EndOfFile,
        ])
        .unwrap();

        assert!(parse_words(&hex).is_err());
    }

    #[test]
    fn packetize_splits_into_chunks_with_correct_addresses() {
        let segment = Segment {
            address: 0x100,
            words: vec![1, 2, 3, 4, 5],
        };
        let packets: Vec<_> = packetize(&segment, 2)
            .map(|(addr, chunk)| (addr, chunk.to_vec()))
            .collect();
        assert_eq!(
            packets,
            vec![(0x100, vec![1, 2]), (0x104, vec![3, 4]), (0x108, vec![5])]
        );
    }
}
