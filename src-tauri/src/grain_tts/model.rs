//! The TTS model adapter: the only file that knows which model Grain speaks
//! with. Today that is Gradium Phonon 31m English (Apache-2.0) with its voice
//! presets, run by Gradium's own `ptts` crate on the pure-Rust `xn` tensor
//! runtime (no ONNX Runtime, no ggml, no phonemizer).
//!
//! To change the model, replace this file: its pinned [`FILES`], its
//! [`VOICES`] and [`Voice`]. Nothing else in Grain imports `ptts` or `xn`.

use crate::grain_model_files::PinnedFile;
use std::path::Path;

/// The default voice (installed with the model): the file stem under the
/// model's `voices/` folder.
pub(crate) const VOICE: &str = "Zoey";

/// The model's folder under the app's `models/` directory.
pub(super) const FOLDER: [&str; 2] = ["grain-tts", "phonon-31m-en"];

/// Pinned to exact Hugging Face commits and verified by size + SHA-256.
pub(super) const FILES: [PinnedFile; 4] = [
    PinnedFile {
        name: "config.json",
        url: "https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f/config.json",
        bytes: 4_634,
        sha256: "b5d0f88353638370d94035b7d73d7d6577c0554d78f8d7ecfd40490aae39ae85",
    },
    PinnedFile {
        name: "tokenizer.json",
        url: "https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f/tokenizer.json",
        bytes: 277_967,
        sha256: "691a00068fa7b989f2f3e5ae5daa2d1a5bb319f2d5360deafcfb19a8bb6e7936",
    },
    PinnedFile {
        name: "voices/Zoey.safetensors",
        url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/NbpkqMVS3CJeq2j8.safetensors",
        bytes: 256_136,
        sha256: "97c7f8c253413574406ab80dd23f49c3ebb2517163b00a9bf86b9c4cf43e7e23",
    },
    // The big one last, so the small files fail fast on a bad connection.
    PinnedFile {
        name: "model.q8.gguf",
        url: "https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f/model.q8.gguf",
        bytes: 64_512_352,
        sha256: "a7d9aad2cd6dc66473f24a08bc6d2939030ef4d751ef82349361fbef3272dbae",
    },
];

/// A talk-back voice the user can pick. Each is one small embedding file,
/// downloaded (pinned and verified) the first time it is picked.
pub(crate) struct VoicePreset {
    pub(crate) name: &'static str,
    /// "us" or "gb".
    pub(crate) accent: &'static str,
    /// "female" or "male".
    pub(crate) gender: &'static str,
    pub(super) file: PinnedFile,
}

const VOICE_BYTES: u64 = 256_136;

/// Seven female and seven male English presets (American and British), from
/// gradium/phonon-voices at a pinned commit. The first is the default.
pub(crate) const VOICES: [VoicePreset; 14] = [
    VoicePreset {
        name: "Zoey",
        accent: "us",
        gender: "female",
        file: PinnedFile {
            name: "voices/Zoey.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/NbpkqMVS3CJeq2j8.safetensors",
            bytes: VOICE_BYTES,
            sha256: "97c7f8c253413574406ab80dd23f49c3ebb2517163b00a9bf86b9c4cf43e7e23",
        },
    },
    VoicePreset {
        name: "Harper",
        accent: "us",
        gender: "female",
        file: PinnedFile {
            name: "voices/Harper.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/4SZHfMpw-p46Ywgs.safetensors",
            bytes: VOICE_BYTES,
            sha256: "6b3da6e2d796a20c51af892c482bd6509fb88267a221aff83c870843dee2a123",
        },
    },
    VoicePreset {
        name: "Hadley",
        accent: "us",
        gender: "female",
        file: PinnedFile {
            name: "voices/Hadley.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/bXwrkHhURL1VFJdG.safetensors",
            bytes: VOICE_BYTES,
            sha256: "a879938aa9a2df0a8220bdadef88137929f71e887bd91e7db205bd5fa11661d2",
        },
    },
    VoicePreset {
        name: "Delia",
        accent: "us",
        gender: "female",
        file: PinnedFile {
            name: "voices/Delia.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/nGzMHMPAWGaJFHOo.safetensors",
            bytes: VOICE_BYTES,
            sha256: "a0c3dcb91b2247eca56536be9ae8e8becc1c807b1136c5fe101552d6a155b24a",
        },
    },
    VoicePreset {
        name: "Maeve",
        accent: "gb",
        gender: "female",
        file: PinnedFile {
            name: "voices/Maeve.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/6PWnV0Nq4wu7RVBT.safetensors",
            bytes: VOICE_BYTES,
            sha256: "56494ee67d533ad227ff0dfd0b1c13ff5aa91f1ad6e596e66c2986ba3005e844",
        },
    },
    VoicePreset {
        name: "Pippa",
        accent: "gb",
        gender: "female",
        file: PinnedFile {
            name: "voices/Pippa.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/uem82D50GRv2Dwma.safetensors",
            bytes: VOICE_BYTES,
            sha256: "d41a8f01d6e33450f1ce062d332914da10beee99e585dab531750706f187c48a",
        },
    },
    VoicePreset {
        name: "Bryony",
        accent: "gb",
        gender: "female",
        file: PinnedFile {
            name: "voices/Bryony.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/gfP44RlHwr2kEwk7.safetensors",
            bytes: VOICE_BYTES,
            sha256: "bcc207a73b7885e45b49ab633d3377a10158292e3a7cef9cd6764ad107f1cb9b",
        },
    },
    VoicePreset {
        name: "Micah",
        accent: "us",
        gender: "male",
        file: PinnedFile {
            name: "voices/Micah.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/40QrMVgAxFiUnP9f.safetensors",
            bytes: VOICE_BYTES,
            sha256: "0de1098d1051686a138d068e83ff09deb5053328bc74ec9541a1fdb90bfc24da",
        },
    },
    VoicePreset {
        name: "Miles",
        accent: "us",
        gender: "male",
        file: PinnedFile {
            name: "voices/Miles.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/d9XCyhjo4JhkHF1d.safetensors",
            bytes: VOICE_BYTES,
            sha256: "eab20e4f159b48cfbc01b1885804e24f681f80e7a64f909182763a4e3361bee8",
        },
    },
    VoicePreset {
        name: "Preston",
        accent: "us",
        gender: "male",
        file: PinnedFile {
            name: "voices/Preston.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/qIxlZ7Ei8KvWlyQH.safetensors",
            bytes: VOICE_BYTES,
            sha256: "6e04b84d3ae8751b22bca0a4f63936f9cb648f4f2182b2d0788071ea1c0b68a1",
        },
    },
    VoicePreset {
        name: "Trevor",
        accent: "us",
        gender: "male",
        file: PinnedFile {
            name: "voices/Trevor.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/0zLApsZfVLLNaEGl.safetensors",
            bytes: VOICE_BYTES,
            sha256: "dc1341c8937af5534e6bbcc5c0fc5e7c10312229b59061228074f93683fe2261",
        },
    },
    VoicePreset {
        name: "Hugh",
        accent: "gb",
        gender: "male",
        file: PinnedFile {
            name: "voices/Hugh.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/YyKcxPG7iIgS3jsK.safetensors",
            bytes: VOICE_BYTES,
            sha256: "3169d74b47a54d1eb15e4c243f56393abe9fe90c7e1c308ecf96ee67618b152a",
        },
    },
    VoicePreset {
        name: "Edwin",
        accent: "gb",
        gender: "male",
        file: PinnedFile {
            name: "voices/Edwin.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/zUCxzrOrugTSis03.safetensors",
            bytes: VOICE_BYTES,
            sha256: "ff1d0735b8b45710462b1d1f846da040f970c6eb62c7c2f36dbfb8e5bde74f1e",
        },
    },
    VoicePreset {
        name: "Teddy",
        accent: "gb",
        gender: "male",
        file: PinnedFile {
            name: "voices/Teddy.safetensors",
            url: "https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7/voices/Zn7g6NxLqXlJb7Qi.safetensors",
            bytes: VOICE_BYTES,
            sha256: "baea0ea9506f3237a5ebfc61afb7c72dbd189c5603b199ef42fbb00dbda379e0",
        },
    },
];

/// Size `xn`'s CPU pool once, at startup. Its default is every core, which on a
/// 20-thread desktop measured 2× SLOWER than 4 threads (the flow LM and Mimi
/// decoder already run on two threads of their own). Called before any other
/// thread exists, because `xn` applies it with `std::env::set_var`.
pub(crate) fn init_threads() {
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    xn::set_num_threads((cores / 2).clamp(1, 4));
}

/// A loaded model with exactly one voice registered.
pub(super) struct Voice {
    synth: ptts::synth::Synth,
    name: String,
}

impl Voice {
    /// Load the model (CPU, Q8) with only `voice` registered; the other
    /// downloaded presets stay on disk.
    pub(super) fn load(dir: &Path, voice: &str) -> Result<Self, String> {
        use ptts::checkpoint::{Checkpoint, ResolveOptions};
        use ptts::preprocess::Lang;
        use ptts::synth::{DeviceKind, Quant};

        let mut checkpoint = Checkpoint::resolve(
            dir,
            ResolveOptions {
                quant: Quant::Q80,
                weights: None,
            },
        )
        .map_err(|e| format!("could not read the voice model: {e}"))?;
        checkpoint.voices.retain(|(name, _)| name == voice);
        let mut synth = checkpoint
            .builder(Lang::En)
            .device(DeviceKind::Cpu)
            .build()
            .map_err(|e| format!("could not load the voice model: {e}"))?;
        checkpoint.register_voices(&mut synth);
        Ok(Self {
            synth,
            name: voice.to_string(),
        })
    }

    pub(super) fn name(&self) -> &str {
        &self.name
    }

    pub(super) fn sample_rate(&self) -> u32 {
        self.synth.sample_rate()
    }

    /// Synthesize `text` as mono f32 PCM chunks at [`Self::sample_rate`].
    /// Dropping the iterator stops and joins the model's workers.
    pub(super) fn stream<'a>(
        &'a self,
        text: &str,
    ) -> Result<impl Iterator<Item = Result<Vec<f32>, String>> + 'a, String> {
        let options = ptts::synth::SpeechOptions::default().voice(&self.name);
        let stream = self
            .synth
            .stream_with(text, &options)
            .map_err(|e| format!("could not start speech: {e}"))?;
        Ok(stream.map(|chunk| chunk.map_err(|e| format!("speech failed: {e}"))))
    }
}
