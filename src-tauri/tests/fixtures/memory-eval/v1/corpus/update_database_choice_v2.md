---
grain_id: 55555555-5555-4555-8555-555555555502
title: Database Migration: Supabase PostgreSQL
timestamp: 1787443200000
tldr: CURRENT DECISION (August 20): Migrated from DynamoDB to PostgreSQL on Supabase for relational queries.
question: What is the current database selection as of August 2026?
reminder: ""
pinned: false
entities:
  - database
  - postgresql
  - supabase
  - migration
  - current_decision
todos: []
source: test_fixture
---

# Architecture Decision: Supabase PostgreSQL (ACTIVE)

Date: 2026-08-20
Supersedes: Architecture Decision: AWS DynamoDB (2026-07-15)

## Decision
We have completely migrated away from DynamoDB to PostgreSQL hosted on Supabase. Complex multi-table filters, relational foreign keys, and vector queries require Postgres.
