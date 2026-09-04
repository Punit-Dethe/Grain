---
grain_id: 66666666-6666-4666-8666-666666666602
title: ASR Inference Engine Deployment
timestamp: 1786492800000
tldr: Guidelines for deploying the speech-to-text / ASR / voice recognition engine.
question: How is the speech-to-text ASR engine deployed?
reminder: ""
pinned: false
entities:
  - asr
  - speech_to_text
  - stt
  - transcribe_cpp
  - voice
todos: []
source: test_fixture
---

# Automatic Speech Recognition (ASR / STT) Engine

The on-device speech-to-text (STT) inference pipeline uses `transcribe-cpp`.

## Principles
- Pure CPU/GPU native inference via Whisper GGUF models.
- Low-RAM streaming buffer ensures zero audio loss during long dictation sessions.
