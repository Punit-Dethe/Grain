//! [GRAIN] File-backed PCM16 storage for one rolling recording.
//!
//! The audio callback appends small resampled frames through a buffered writer.
//! Chunk jobs carry only frame ranges; the serial worker reuses one read buffer.
//! `TempPath` removes the journal when the last session owner is dropped.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

const BYTES_PER_FRAME: u64 = 2;

struct WriterState {
    file: BufWriter<File>,
    encoded: Vec<u8>,
    frames: u64,
}

pub(crate) struct PcmJournal {
    path: tempfile::TempPath,
    writer: Mutex<WriterState>,
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
            }),
        })
    }

    pub(crate) fn append(&self, samples: &[i16]) -> std::io::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.encoded.clear();
        writer.encoded.reserve(samples.len().saturating_mul(2));
        for sample in samples {
            writer.encoded.extend_from_slice(&sample.to_le_bytes());
        }
        let encoded = std::mem::take(&mut writer.encoded);
        writer.file.write_all(&encoded)?;
        writer.encoded = encoded;
        writer.frames = writer.frames.saturating_add(samples.len() as u64);
        Ok(())
    }

    pub(crate) fn flush(&self) -> std::io::Result<()> {
        self.writer.lock().unwrap().file.flush()
    }

    pub(crate) fn frame_count(&self) -> u64 {
        self.writer.lock().unwrap().frames
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
}
