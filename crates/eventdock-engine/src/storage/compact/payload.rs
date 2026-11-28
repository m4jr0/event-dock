//! Contiguous FIFO payload allocation within a fixed physical ring.

#[derive(Debug)]
pub(super) struct PayloadBuffer {
    bytes: Vec<u8>,
    head: usize,
    tail: usize,
    live_bytes: usize,
}

impl PayloadBuffer {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            bytes: vec![0; capacity],
            head: 0,
            tail: 0,
            live_bytes: 0,
        }
    }

    // Find contiguous space without changing the buffer on failure.
    fn placement(
        &self,
        len: usize,
    ) -> Option<usize> {
        if len == 0 || len > self.bytes.len() - self.live_bytes {
            return None;
        }
        if self.live_bytes == 0 {
            return Some(0);
        }
        if self.tail <= self.head {
            return (len <= self.head - self.tail).then_some(self.tail);
        }
        if len <= self.bytes.len() - self.tail {
            return Some(self.tail);
        }
        (len <= self.head).then_some(0)
    }

    pub(super) fn push(
        &mut self,
        payload: &[u8],
    ) -> Option<usize> {
        let offset = self.placement(payload.len())?;
        let end = offset + payload.len();
        self.bytes[offset..end].copy_from_slice(payload);
        if self.live_bytes == 0 {
            self.head = offset;
        }
        self.tail = end;
        self.live_bytes += payload.len();
        Some(offset)
    }

    // Release the oldest record and use the next record's offset to skip padding.
    // The owner supplies FIFO metadata; no record directory is duplicated here.
    pub(super) fn release(
        &mut self,
        offset: usize,
        len: usize,
        next: Option<usize>,
    ) {
        assert_eq!(offset, self.head);
        assert!(len > 0 && len <= self.live_bytes);
        assert!(len <= self.bytes.len() - offset);
        let remaining = self.live_bytes - len;
        assert_eq!(next.is_none(), remaining == 0);
        if let Some(next) = next {
            assert!(next < self.bytes.len());
            assert!(next == offset + len || next == 0);
            self.head = next;
        } else {
            self.head = 0;
            self.tail = 0;
        }
        self.live_bytes = remaining;
    }

    pub(super) fn live_bytes(&self) -> usize {
        self.live_bytes
    }

    pub(super) fn capacity(&self) -> usize {
        self.bytes.len()
    }

    pub(super) fn allocated_bytes(&self) -> usize {
        self.bytes.capacity()
    }

    #[cfg(test)]
    pub(super) fn truncate_for_test(
        &mut self,
        len: usize,
    ) {
        self.bytes.truncate(len);
    }

    // Read a retained record using its owner's metadata.
    pub(super) fn get(
        &self,
        offset: usize,
        len: usize,
    ) -> Option<&[u8]> {
        self.bytes
            .get(offset..offset.checked_add(len)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn wraps_without_moving_retained_records() {
        let mut ring = PayloadBuffer::new(10);
        assert_eq!(ring.push(b"abcd"), Some(0));
        assert_eq!(ring.push(b"efgh"), Some(4));
        ring.release(0, 4, Some(4));
        assert_eq!(ring.push(b"ijk"), Some(0));
        assert_eq!(ring.get(4, 4), Some(b"efgh".as_slice()));
        assert_eq!(ring.push(b"zz"), None);
        ring.release(4, 4, Some(0));
        assert_eq!(ring.push(b"zz"), Some(3));
        assert_eq!(ring.get(0, 3), Some(b"ijk".as_slice()));
    }

    #[test]
    fn exact_capacity_and_empty_reset() {
        let mut ring = PayloadBuffer::new(4);
        assert_eq!(ring.push(b"abcd"), Some(0));
        assert_eq!(ring.push(b"x"), None);
        ring.release(0, 4, None);
        assert_eq!(ring.push(b"xy"), Some(0));
        assert_eq!(ring.get(0, 2), Some(b"xy".as_slice()));
        assert_eq!(ring.push(b""), None);
        assert_eq!(ring.get(usize::MAX, 2), None);
    }

    #[test]
    fn padding_does_not_make_equal_head_and_tail_writable() {
        let mut ring = PayloadBuffer::new(10);
        assert_eq!(ring.push(b"abcd"), Some(0));
        assert_eq!(ring.push(b"efgh"), Some(4));
        ring.release(0, 4, Some(4));
        assert_eq!(ring.push(b"ijkl"), Some(0));
        assert_eq!(ring.push(b"x"), None);
        assert_eq!(ring.get(4, 4), Some(b"efgh".as_slice()));
        assert_eq!(ring.get(0, 4), Some(b"ijkl".as_slice()));
    }

    #[test]
    fn variable_records_preserve_fifo_contents() {
        let mut ring = PayloadBuffer::new(127);
        let mut records: VecDeque<(usize, Vec<u8>)> = VecDeque::new();
        let mut state = 12345_u32;
        for _ in 0..10000 {
            state = state
                .wrapping_mul(1664525)
                .wrapping_add(1013904223);
            let len = (state as usize % 127) + 1;
            let payload = vec![(state >> 16) as u8; len];
            loop {
                if let Some(offset) = ring.push(&payload) {
                    records.push_back((offset, payload));
                    break;
                }
                let (offset, old) = records.pop_front().unwrap();
                ring.release(
                    offset,
                    old.len(),
                    records.front().map(|record| record.0),
                );
            }
            for (offset, expected) in &records {
                assert_eq!(
                    ring.get(*offset, expected.len()),
                    Some(expected.as_slice())
                );
            }
            assert_eq!(
                ring.live_bytes,
                records
                    .iter()
                    .map(|record| record.1.len())
                    .sum::<usize>()
            );
        }
    }
}
