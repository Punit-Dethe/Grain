//! [GRAIN] File-backed PCM16 storage for one rolling recording.
//!
//! The audio callback appends small resampled frames through a buffered writer.
//! Chunk jobs carry only frame ranges; the serial worker reuses one read buffer.
//! `TempPath` removes the journal when the last session owner is dropped.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Condvar, Mutex,
};
use std::time::Duration;

const BYTES_PER_FRAME: u64 = 2;

struct WriterState {
    file: BufWriter<File>,
    encoded: Vec<u8>,
    frames: u64,
    closed: bool,
}

pub(crate) struct PcmJournal {
    path: tempfile::TempPath,
    writer: Mutex<WriterState>,
    availability: Condvar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JournalAvailability {
    Available,
    Closed,
    Cancelled,
}

pub(crate) struct PcmJournalReader {
    file: File,
    encoded: Vec<u8>,
}

impl PcmJournal {
    pub(crate) fn create() -> std::io::Result<Self> {
        let named = tempfile::Builder::new()
            .prefix("grain-rolling-")
            .suffix(".pcm16")
            .tempfile()?;
        let (file, path) = named.into_parts();
        Ok(Self {
            path,
            writer: Mutex::new(WriterState {
                file: BufWriter::with_capacity(64 * 1024, file),
                encoded: Vec::with_capacity(960),
                frames: 0,
                closed: false,
            }),
            availability: Condvar::new(),
        })
    }

    pub(crate) fn append(&self, samples: &[i16]) -> std::io::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        if writer.closed {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "PCM journal is closed",
            ));
        }
        writer.encoded.clear();
        writer.encoded.reserve(samples.len().saturating_mul(2));
        for sample in samples {
            writer.encoded.extend_from_slice(&sample.to_le_bytes());
        }
        let encoded = std::mem::take(&mut writer.encoded);
        writer.file.write_all(&encoded)?;
        writer.encoded = encoded;
        writer.frames = writer.frames.saturating_add(samples.len() as u64);
        drop(writer);
        self.availability.notify_all();
        Ok(())
    }

    /// Wait until a TDT descriptor's bounded right-context tail is readable.
    /// Generic rolling never calls this method and retains its prior behavior.
    pub(crate) fn wait_for_frames(
        &self,
        target: u64,
        cancelled: &AtomicBool,
    ) -> std::io::Result<JournalAvailability> {
        let mut writer = self.writer.lock().unwrap();
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Ok(JournalAvailability::Cancelled);
            }
            if writer.frames >= target {
                writer.file.flush()?;
                return Ok(JournalAvailability::Available);
            }
            if writer.closed {
                writer.file.flush()?;
                return Ok(JournalAvailability::Closed);
            }
            let (next, _) = self
                .availability
                .wait_timeout(writer, Duration::from_millis(50))
                .unwrap();
            writer = next;
        }
    }

    pub(crate) fn close(&self) -> std::io::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.file.flush()?;
        writer.closed = true;
        drop(writer);
        self.availability.notify_all();
        Ok(())
    }

    pub(crate) fn wake_waiters(&self) {
        self.availability.notify_all();
    }

    pub(crate) fn flush(&self) -> std::io::Result<()> {
        self.writer.lock().unwrap().file.flush()
    }

    pub(crate) fn frame_count(&self) -> u64 {
        self.writer.lock().unwrap().frames
    }

    pub(crate) fn byte_len(&self) -> u64 {
        self.frame_count().saturating_mul(BYTES_PER_FRAME)
    }

    pub(crate) fn reader(&self) -> std::io::Result<PcmJournalReader> {
        self.flush()?;
        Ok(PcmJournalReader {
            file: File::open(&self.path)?,
            encoded: Vec::new(),
        })
    }

    pub(crate) fn read_all_f32(&self) -> std::io::Result<Vec<f32>> {
        let frames = self.frame_count();
        let mut output = Vec::with_capacity(frames.min(usize::MAX as u64) as usize);
        self.reader()?.read_f32_range(0, frames, &mut output)?;
        Ok(output)
    }

    pub(crate) fn save_wav(&self, path: &Path) -> anyhow::Result<()> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut wav = hound::WavWriter::create(path, spec)?;
        let mut reader = self.reader()?;
        let mut samples = Vec::new();
        let total = self.frame_count();
        let mut start = 0u64;
        while start < total {
            let end = (start + 32_768).min(total);
            reader.read_f32_range(start, end, &mut samples)?;
            for sample in &samples {
                wav.write_sample((sample * 32768.0).clamp(-32768.0, 32767.0) as i16)?;
            }
            start = end;
        }
        wav.finalize()?;
        Ok(())
    }

    #[cfg(test)]
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl PcmJournalReader {
    pub(crate) fn read_f32_range(
        &mut self,
        start_frame: u64,
        end_frame: u64,
        output: &mut Vec<f32>,
    ) -> std::io::Result<()> {
        let frames = end_frame.saturating_sub(start_frame) as usize;
        let bytes = frames.saturating_mul(BYTES_PER_FRAME as usize);
        self.encoded.resize(bytes, 0);
        self.file
            .seek(SeekFrom::Start(start_frame.saturating_mul(BYTES_PER_FRAME)))?;
        self.file.read_exact(&mut self.encoded)?;
        output.clear();
        if output.capacity() < frames {
            output.reserve(frames);
        }
        output.extend(
            self.encoded
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_reads_are_exact_and_reuse_output_capacity() {
        let journal = PcmJournal::create().unwrap();
        journal.append(&[-32768, -1, 0, 1, 32767]).unwrap();
        let mut reader = journal.reader().unwrap();
        let mut output = Vec::new();
        reader.read_f32_range(1, 4, &mut output).unwrap();
        let capacity = output.capacity();
        assert_eq!(output, vec![-1.0 / 32768.0, 0.0, 1.0 / 32768.0]);
        reader.read_f32_range(2, 4, &mut output).unwrap();
        assert_eq!(output, vec![0.0, 1.0 / 32768.0]);
        assert_eq!(output.capacity(), capacity);
    }

    #[test]
    fn journal_file_is_removed_on_drop() {
        let journal = PcmJournal::create().unwrap();
        let path = journal.path().to_path_buf();
        assert!(path.exists());
        drop(journal);
        assert!(!path.exists());
    }

    #[test]
    fn lookahead_wait_reports_available_closed_and_cancelled() {
        let journal = PcmJournal::create().unwrap();
        let cancelled = AtomicBool::new(false);
        journal.append(&[1, 2, 3]).unwrap();
        assert_eq!(
            journal.wait_for_frames(3, &cancelled).unwrap(),
            JournalAvailability::Available
        );
        journal.close().unwrap();
        assert_eq!(
            journal.wait_for_frames(4, &cancelled).unwrap(),
            JournalAvailability::Closed
        );

        let other = PcmJournal::create().unwrap();
        cancelled.store(true, Ordering::Release);
        assert_eq!(
            other.wait_for_frames(1, &cancelled).unwrap(),
            JournalAvailability::Cancelled
        );
    }

    #[test]
    fn close_is_durable_and_rejects_late_appends() {
        let journal = PcmJournal::create().unwrap();
        journal.append(&[7, 8]).unwrap();
        journal.close().unwrap();
        assert_eq!(journal.read_all_f32().unwrap().len(), 2);
        assert_eq!(
            journal.append(&[9]).unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn repeated_appends_reuse_scratch_and_report_exact_bytes() {
        let journal = PcmJournal::create().unwrap();
        let block = vec![123i16; 960];

        journal.append(&block).unwrap();
        let capacity = journal.writer.lock().unwrap().encoded.capacity();
        for _ in 1..1_000 {
            journal.append(&block).unwrap();
            assert_eq!(journal.writer.lock().unwrap().encoded.capacity(), capacity);
        }

        assert_eq!(journal.frame_count(), 960_000);
        assert_eq!(journal.byte_len(), 1_920_000);
    }
}
