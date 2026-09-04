---
grain_id: 22222222-2222-4222-8222-222222222203
title: Weekly Team Sync - Week 34
timestamp: 1787529600000
tldr: Team sync Week 34: Vector index latency evaluation and BGE model caching.
question: What were the findings on vector index latency in Week 34 sync?
reminder: ""
pinned: false
entities:
  - weekly_sync
  - team_meeting
  - vector_index
  - bge_embed
  - latency
todos: []
source: test_fixture
---

# Weekly Team Sync - Week 34 (2026-08-21)

Attendees: Alice, Bob, Charlie, Punit

## Agenda & Discussion
1. Alice shared evaluation numbers for BGE-small embedding model on CPU.
2. Latency: Query embedding takes ~8ms; cosine search against 1,000 vectors takes ~1.2ms.
3. Agreed that BGE model must be unloaded when Notes tab unmounts to preserve zero-idle-RAM invariant.

## Action Items
- [ ] Alice: Ensure `shutdown_engine_if_idle` is called consistently on UI tab change.
