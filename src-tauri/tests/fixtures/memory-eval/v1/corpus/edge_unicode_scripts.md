---
grain_id: 88888888-8888-4888-8888-888888888802
title: Multilingual Unicode and Math Spec
timestamp: 1786924800000
tldr: Document with diverse scripts: 中文, 日本語, العربية, Русский, math formulas and emojis.
question: How does search handle multilingual unicode and mathematical notations?
reminder: ""
pinned: false
entities:
  - unicode
  - multilingual
  - chinese
  - arabic
  - emoji
  - math
todos: []
source: test_fixture
---

# Multilingual & Math Test Note

Testing tokenization, FTS5 Unicode61 tokenizer, and vector chunking across diverse language scripts:

- 中文 (Simplified): 这是一个测试笔记，用于验证全文检索的字符切分。
- 日本語: 日本語の自然言語処理と形態素解析の検証用テキストです。
- العربية: هذا اختبار للتحقق من دعم النصوص المكتوبة من اليمين إلى اليسار.
- Русский: Проверка поддержки кириллицы в полнотекстовом поиске Grain.
- Math Symbols: $\int_{0}^{\infty} e^{-x^2} dx = \frac{\sqrt{\pi}}{2}$, $\forall x \in \mathbb{R}$.
- Emojis: 🚀 🔐 💡 🧠 ⚡️
