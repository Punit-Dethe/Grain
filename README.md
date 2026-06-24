<div align="center">
  <img src="src-tauri/icons/128x128.png" alt="Grain Logo" width="128" height="128" />
  <h1>Grain</h1>
</div>

**A free, open source, and extensible speech-to-text application that works completely offline.**

*Grain is a hard fork built upon the excellent foundation of [Handy](https://github.com/cjpais/handy).*

Grain is a cross-platform desktop application that provides simple, privacy-focused speech transcription. Press a shortcut, speak, and have your words appear in any text field. This happens on your own computer without sending any information to the cloud (unless you opt into external API providers).

## Why Grain?

Grain was created to fill the gap for a truly open source, extensible speech-to-text tool:

- **Free**: Accessibility tooling belongs in everyone's hands, not behind a paywall
- **Open Source**: Together we can build further. Extend Grain for yourself and contribute to something bigger
- **Private**: Your voice stays on your computer. Get transcriptions without sending audio to the cloud
- **Simple**: One tool, one job. Transcribe what you say and put it into a text box

Grain isn't trying to be the best speech-to-text app—it's trying to be the most forkable one.

## What's New in Grain?

We've massively upgraded the core capabilities to make transcription faster and more powerful:

- **Rolling Window Dictation**: Real-time transcription using a live rolling model.
- **Agent Workflow**: Leverage smart agent processing on your dictated text.
- **On-the-Fly Model Switching**: Switch models seamlessly while speaking.
- **Radically Lower Memory Footprint**: We decoupled the frontend from the backend, slashing RAM usage (Peak: ~40MB, Average: ~30MB).
- **OpenAI Compatible STT**: The Speech-to-Text endpoint is now fully OpenAI compatible.
- **Multi-Provider & Smart Rotation**: Support for multiple STT and Post-Processing providers. Grain will smartly rotate between them based on daily limits, round-robin rules, and availability. Both endpoints feature this smart rotation!
- **Quick Panel**: Access your most important settings instantly from a beautiful, responsive quick panel—no need to dig through full settings windows.

## How It Works

1. **Press** a configurable keyboard shortcut to start/stop recording (or use push-to-talk mode)
2. **Speak** your words while the shortcut is active
3. **Release** and Grain processes your speech using Whisper or Parakeet
4. **Get** your transcribed text pasted directly into whatever app you're using

The core process can be run entirely locally:

- Silence is filtered using VAD (Voice Activity Detection) with Silero
- Transcription uses your choice of models:
  - **Whisper models** (Small/Medium/Turbo/Large) with GPU acceleration when available
  - **Parakeet V3** - CPU-optimized model with excellent performance and automatic language detection
- Works on Windows, macOS, and Linux

## Quick Start

### Installation

1. Download the latest release from the [releases page](https://github.com/Punit-Dethe/Grain/releases).
2. Install the application
3. Launch Grain and grant necessary system permissions (microphone, accessibility)
4. Configure your preferred keyboard shortcuts in Settings
5. Start transcribing!

### Development Setup

For detailed build instructions including platform-specific requirements, see [BUILD.md](BUILD.md).

## Architecture

Grain is built as a Tauri application combining:

- **Frontend**: React + TypeScript with Tailwind CSS for the settings UI and Quick Panel
- **Backend**: Rust for system integration, audio processing, and ML inference
- **Core Libraries**:
  - `whisper-rs`: Local speech recognition with Whisper models
  - `transcribe-rs`: CPU-optimized speech recognition with Parakeet models
  - `cpal`: Cross-platform audio I/O
  - `vad-rs`: Voice Activity Detection
  - `rdev`: Global keyboard shortcuts and system events
  - `rubato`: Audio resampling

### Debug Mode

Grain includes an advanced debug mode for development and troubleshooting. Access it by pressing:

- **macOS**: `Cmd+Shift+D`
- **Windows/Linux**: `Ctrl+Shift+D`

### CLI Parameters

Grain supports command-line flags for controlling a running instance and customizing startup behavior. These work on all platforms (macOS, Windows, Linux).

**Remote control flags** (sent to an already-running instance via the single-instance plugin):

```bash
grain-core --toggle-transcription    # Toggle recording on/off
grain-core --toggle-post-process     # Toggle recording with post-processing on/off
grain-core --cancel                  # Cancel the current operation
```

**Startup flags:**

```bash
grain-core --start-hidden            # Start without showing the main window
grain-core --no-tray                 # Start without the system tray icon
grain-core --debug                   # Enable debug mode with verbose logging
grain-core --help                    # Show all available flags
```

## Known Issues & Current Limitations

### Major Issues (Help Wanted)

**Whisper Model Crashes:**

- Whisper models crash on certain system configurations (Windows and Linux)
- Does not affect all systems - issue is configuration-dependent
  - If you experience crashes and are a developer, please help to fix and provide debug logs!

**Wayland Support (Linux):**

- Limited support for Wayland display server
- Requires `wtype` or `dotool` for text input to work correctly.
