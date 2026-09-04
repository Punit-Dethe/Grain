---
grain_id: 22222222-2222-4222-8222-222222222204
title: Weekly Team Sync - Week 35
timestamp: 1788134400000
tldr: Team sync Week 35: Deprecating unrestricted note update and planning memory system.
question: What architectural changes were decided in the Week 35 team sync?
reminder: ""
pinned: false
entities:
  - weekly_sync
  - team_meeting
  - memory_system
  - append_safety
  - roadmap
todos: []
source: test_fixture
---

# Weekly Team Sync - Week 35 (2026-08-28)

Attendees: Alice, Bob, Charlie, Punit

## Agenda & Discussion
1. Team consensus: Unrestricted `update_note(id, body)` is dangerous and causes silent note clobbering.
2. Decision: Transition to revision-bound append target tokens and immutable block ledgers.
3. Punit will formalize the complete plan in `docs/Grain Space 2.0/MEMORY-SYSTEM-PLAN.md`.

## Action Items
- [ ] Punit: Complete MEMORY-SYSTEM-PLAN.md and establish Phase 0 evaluation corpus.
