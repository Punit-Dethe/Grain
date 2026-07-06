# BGE-small-en-v1.5

**Status:** Chosen embedding model for local semantic search

---

# Why this model?

This application performs semantic search over personal notes.

Each note consists of:

- 3-word title
- 1 sentence TLDR
- full body

Requirements:

- Local inference only
- Rust
- Candle
- Very low RAM
- Fast startup
- Fast query latency
- High semantic quality

After comparing current embedding models, **BGE-small-en-v1.5** provides the best balance of:

- semantic quality
- memory usage
- startup speed
- CPU inference speed

---

# Model Information

Model:

```
BAAI/bge-small-en-v1.5
```

Official model:

https://huggingface.co/BAAI/bge-small-en-v1.5

License

MIT

Commercial use

Yes

Architecture

Encoder-only Transformer (BERT family)

Parameters

```
33.4 Million
```

Embedding dimension

```
384
```

Maximum input length

```
512 tokens
```

Language

```
English
```

---

# Why not larger models?

Larger models improve retrieval quality slightly but increase RAM dramatically.

| Model | Approx Runtime RAM |
|---------|------------------:|
| MiniLM-L6 | 35 MB |
| **BGE-small** | **45–60 MB** |
| BGE-base | 130–170 MB |
| BGE-M3 | 700+ MB |

For a personal notes application, BGE-small sits almost exactly on the quality/performance sweet spot.

---

# Expected Memory Usage

Using Candle with INT8 quantization.

| Component | RAM |
|-----------|----:|
| Model weights | ~33 MB |
| Tokenizer | ~2 MB |
| Runtime activations | ~8 MB |
| Allocator overhead | ~5 MB |

Expected total:

```
45–60 MB RSS
```

---

# Startup Time

Cold start

```
60–120 ms
```

Warm startup

```
Instant
```

The model should remain loaded for the lifetime of the application.

---

# Search Latency

Typical query

```
2–5 ms
```

Embedding a note

```
3–8 ms
```

Searching

10,000 notes

```
<2 ms
```

Total

```
5–10 ms
```

---

# Embedding Format

Dimension

```
384
```

Similarity

```
Cosine Similarity
```

Normalization

```
Required
```

Always L2-normalize embeddings before storing or searching.

---

# Recommended Note Format

Instead of embedding fields independently, combine them.

Example:

```
Shopping List

Need groceries after work.

Need to buy milk, eggs, bread, cheese,
coffee, and fruit.
```

Equivalent string:

```
Title: Shopping List

Summary:
Need groceries after work.

Body:
Need to buy milk, eggs, bread,
cheese, coffee, and fruit.
```

One embedding per note.

---

# Search Pipeline

User query

↓

Embed query

↓

Cosine similarity

↓

Top K results

↓

(Optional reranker later)

---

# Storage

Store:

```
id
title
summary
body
embedding
```

Embedding

```
384 float32 values
```

Storage per note

```
384 × 4 bytes
≈1536 bytes
```

Example

10,000 notes

```
≈15 MB
```

If embeddings are stored as float16

```
≈7.5 MB
```

---

# Model Loading

Load once.

Never unload unless the application exits.

```
Application Start

↓

Load tokenizer

↓

Load model

↓

Keep in memory

↓

Embed notes

↓

Embed queries

↓

Shutdown
```

---

# Recommended Quantization

```
INT8
```

Reason

- minimal accuracy loss
- low RAM
- excellent CPU performance

---

# Recommended Search

Cosine similarity

Do not use Euclidean distance.

---

# Future Improvements

Possible additions later:

- Hybrid BM25 + embeddings
- Cross-encoder reranker
- Incremental embedding updates
- Metadata filtering
- Tag-aware search
- Recency boosting
- Personalized ranking

None of these require changing the embedding model.

---

# Why This Model Was Chosen

Pros

✓ Excellent semantic understanding

✓ Very small

✓ Very fast

✓ Low memory

✓ Excellent CPU inference

✓ Works well in Rust

✓ MIT license

✓ Mature ecosystem

Cons

- English only
- 512-token context
- Slightly lower retrieval quality than much larger models

---

# Final Decision

Model

```
BAAI/bge-small-en-v1.5
```

Embedding size

```
384
```

Runtime RAM

```
45–60 MB
```

Quantization

```
INT8
```

Framework

```
Candle
```

Similarity

```
Cosine
```

Status

```
Production Choice
```