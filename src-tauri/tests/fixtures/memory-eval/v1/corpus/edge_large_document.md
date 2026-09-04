---
grain_id: 88888888-8888-4888-8888-888888888803
title: Large Architecture Specification Document
timestamp: 1787011200000
tldr: Extensive architectural document testing chunking, token limits, and excerpting.
question: What are the subsystems detailed in the large architecture spec?
reminder: ""
pinned: false
entities:
  - large_doc
  - architecture
  - chunking
  - subsystems
  - performance
todos: []
source: test_fixture
---

# Extensive Grain Architecture Specification

## Section 1: Memory Architecture & Local Storage
Grain Space uses plain Markdown files with YAML frontmatter as the durable, user-owned representation of every note. The local storage system avoids all proprietary binary formats. An application-wide SQLite database provides derived full-text search (FTS5) and optional local vector embeddings via sqlite-vec.

## Section 2: Concurrency & Lock Management
A single application-wide Mutex serializes every vault read, write, and index operation. In-memory connection handles are dropped immediately after each query to minimize resident RAM.

## Section 3: Audio Ingestion & VAD Pipeline
The voice subsystem buffers PCM frames in memory using a rolling window ring buffer. Silero VAD detects voice activity boundaries without leaking memory.

## Section 4: Transcription Engine
transcribe-cpp runs GGUF quantized models with SIMD vectorization on x86_64 and Apple Silicon.

## Section 5: Reranking & Retrieval Fusion
Reciprocal Rank Fusion (RRF) combines lexical BM25 ranks, vector similarity cosine ranks, and entity graph traversal scores. A deterministic CPU reranker breaks ties using term overlap fraction and recency decay.

## Section 6: Action Execution & Confirmation
Risky mutations trigger host-held prepared calls requiring explicit user confirmation.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.

Additional architecture commentary and specifications padding to verify large document handling across chunk boundaries.
