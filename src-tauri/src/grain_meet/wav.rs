//! Crash-tolerant float32 WAV chunk files.
//!
//! A fixed 44-byte IEEE-float header precedes interleaved little-endian f32
//! frames. A checkpoint flushes buffered frames and rewrites the header sizes,
//! so after a process crash the file is readable up to its last checkpoint and
//! [`salvage`] can recover every whole frame the OS received. Only sealing
//! synchronizes the file to disk.

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

pub const HEADER_LEN: u64 = 44;
const BYTES_PER_SAMPLE: u64 = 4;
/// `WAVE_FORMAT_IEEE_FLOAT`.
const FORMAT_FLOAT: u16 = 3;

fn header(sample_rate: u32, channels: u16, data_bytes: u32) -> [u8; HEADER_LEN as usize] {
    let block_align = u32::from(channels) * BYTES_PER_SAMPLE as u32;
    let mut h = [0u8; HEADER_LEN as usize];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36u32.saturating_add(data_bytes)).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&FORMAT_FLOAT.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&(sample_rate.saturating_mul(block_align)).to_le_bytes());
    h[32..34].copy_from_slice(&(block_align as u16).to_le_bytes());
    h[34..36].copy_from_slice(&32u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

fn data_bytes(frames: u64, channels: u16) -> io::Result<u32> {
    frames
        .checked_mul(u64::from(channels) * BYTES_PER_SAMPLE)
        .and_then(|bytes| u32::try_from(bytes).ok())
        .filter(|bytes| u64::from(*bytes) + HEADER_LEN - 8 <= u64::from(u32::MAX))
        .ok_or_else(|| io::Error::other("WAV chunk exceeds the 4 GiB format limit"))
}

/// An open chunk being appended to.
pub struct ChunkFile {
    out: BufWriter<File>,
    sample_rate: u32,
    channels: u16,
    frames: u64,
}

impl ChunkFile {
    /// Create a new chunk. Fails if `path` already exists.
    pub fn create(path: &Path, sample_rate: u32, channels: u16) -> io::Result<Self> {
        if sample_rate == 0 || channels == 0 {
            return Err(io::Error::other(
                "WAV chunk needs a sample rate and channels",
            ));
        }
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let mut out = BufWriter::new(file);
        out.write_all(&header(sample_rate, channels, 0))?;
        Ok(Self {
            out,
            sample_rate,
            channels,
            frames: 0,
        })
    }

    /// Append interleaved frames. The sample count must be whole frames.
    pub fn append(&mut self, samples: &[f32]) -> io::Result<()> {
        let channels = usize::from(self.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(io::Error::other(
                "audio block is not a whole number of frames",
            ));
        }
        let frames = (samples.len() / channels) as u64;
        data_bytes(self.frames + frames, self.channels)?;
        for sample in samples {
            self.out.write_all(&sample.to_le_bytes())?;
        }
        self.frames += frames;
        Ok(())
    }

    /// Make the frames written so far readable after a process crash.
    pub fn checkpoint(&mut self) -> io::Result<()> {
        self.out.flush()?;
        let bytes = data_bytes(self.frames, self.channels)?;
        let file = self.out.get_mut();
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header(self.sample_rate, self.channels, bytes))?;
        file.seek(SeekFrom::End(0))?;
        Ok(())
    }

    /// Final header and a durable sync. Returns the frame count.
    pub fn seal(mut self) -> io::Result<u64> {
        self.checkpoint()?;
        self.out.get_ref().sync_all()?;
        Ok(self.frames)
    }
}

/// Recover a chunk left open by a crash: keep every whole frame present on
/// disk, drop a trailing partial frame, rewrite the header and sync. The
/// header supplies the format, so a missing or foreign header is refused.
pub fn salvage(path: &Path, sample_rate: u32, channels: u16) -> io::Result<u64> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let mut existing = [0u8; HEADER_LEN as usize];
    let len = file.metadata()?.len();
    if len < HEADER_LEN {
        return Err(io::Error::other("chunk ends inside its WAV header"));
    }
    io::Read::read_exact(&mut file, &mut existing)?;
    let expected = header(sample_rate, channels, 0);
    // Everything but the two size fields must match what was created.
    if existing[8..40] != expected[8..40] || existing[0..4] != expected[0..4] {
        return Err(io::Error::other(
            "chunk header does not match its manifest format",
        ));
    }
    let frame_bytes = u64::from(channels) * BYTES_PER_SAMPLE;
    let frames = (len - HEADER_LEN) / frame_bytes;
    file.set_len(HEADER_LEN + frames * frame_bytes)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&header(
        sample_rate,
        channels,
        data_bytes(frames, channels)?,
    ))?;
    file.sync_all()?;
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> (hound::WavSpec, Vec<f32>) {
        let reader = hound::WavReader::open(path).unwrap();
        let spec = reader.spec();
        (
            spec,
            reader.into_samples::<f32>().map(Result::unwrap).collect(),
        )
    }

    #[test]
    fn sealed_chunk_is_standard_float_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("000000.wav");
        let mut chunk = ChunkFile::create(&path, 48_000, 2).unwrap();
        chunk.append(&[0.5, -0.5, 0.25, -0.25]).unwrap();
        assert!(chunk.append(&[0.1]).is_err(), "half a stereo frame");
        assert_eq!(chunk.seal().unwrap(), 2);
        let (spec, samples) = read(&path);
        assert_eq!(
            (spec.sample_rate, spec.channels, spec.bits_per_sample),
            (48_000, 2, 32)
        );
        assert_eq!(spec.sample_format, hound::SampleFormat::Float);
        assert_eq!(samples, vec![0.5, -0.5, 0.25, -0.25]);
        assert!(
            ChunkFile::create(&path, 48_000, 2).is_err(),
            "never overwrites"
        );
    }

    #[test]
    fn checkpoint_makes_an_open_chunk_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("open.wav.partial");
        let mut chunk = ChunkFile::create(&path, 16_000, 1).unwrap();
        chunk.append(&[0.1, 0.2, 0.3]).unwrap();
        chunk.checkpoint().unwrap();
        chunk.append(&[0.4]).unwrap();
        assert_eq!(read(&path).1, vec![0.1, 0.2, 0.3]);
        drop(chunk);
    }

    #[test]
    fn salvage_keeps_whole_frames_and_drops_a_torn_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("torn.wav.partial");
        let mut chunk = ChunkFile::create(&path, 16_000, 2).unwrap();
        chunk.append(&[0.1, 0.2, 0.3, 0.4]).unwrap();
        chunk.checkpoint().unwrap();
        drop(chunk);
        // A crash mid-write: one more full frame reached the OS, plus 6 bytes.
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        for sample in [0.5f32, 0.6] {
            file.write_all(&sample.to_le_bytes()).unwrap();
        }
        file.write_all(&[0u8; 6]).unwrap();
        drop(file);
        assert_eq!(salvage(&path, 16_000, 2).unwrap(), 3);
        assert_eq!(read(&path).1, vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
        assert!(
            salvage(&path, 48_000, 2).is_err(),
            "format mismatch is refused"
        );

        let short = dir.path().join("short.wav.partial");
        std::fs::write(&short, b"RIFF").unwrap();
        assert!(salvage(&short, 16_000, 2).is_err());
    }
}
