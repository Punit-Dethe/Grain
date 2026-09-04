---
grain_id: 88888888-8888-4888-8888-888888888806
title: Malformed Frontmatter Note
timestamp: invalid_timestamp_here
this is not valid yaml: [unclosed array
---

# Malformed Frontmatter Document

This document has broken YAML syntax in the frontmatter block.
Grain must not crash when encountering this file, and should safely degrade to treating the body as readable text.
