---
grain_id: "m_edge_malformed_35"
title: "Malformed frontmatter resilience test"
tldr: "Edge test case with trailing unclosed frontmatter and raw delimiters."
question: "how does Grain handle malformed frontmatter delimiters?"
entities: [edge case, malformed markdown, parser resilience]
created: 2026-07-25T10:00:00.000
---
This note contains irregular markdown text to test parser fault-tolerance.
Here is a fake delimiter line in the middle of text:
---
The parser must not truncate the text following the fake delimiter line above.
It should preserve all body text intact:
- Item alpha
- Item beta with colon: and quote "
- URL test: https://resilience.example.org/test
