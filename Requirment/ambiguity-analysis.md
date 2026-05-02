# Initial Requirements — Ambiguity & Gap Analysis

Analysis of `initial reqs.md` against `First Specification.md` and `Bango Platform Analysis.md`. Each item is a concrete ambiguity, contradiction, or missing specification that must be resolved before development.

---

## 1. Broken / Incomplete Sentences

| # | Location | Issue | Resolution Needed |
|---|----------|-------|-------------------|
| 1.1 | Line 24 | `"allow users to move articles between"` — sentence is cut off. | Complete the sentence. Presumably "between working, included, and rejected lists", but the exact scope matters: can users also move articles back to imported? Can included articles be demoted to rejected without going through working first? |

---

## 2. Undefined or Conflicting Terminology

### 2.1 Tags vs Labels vs Meta-tags

The initial requirements use three distinct terms without defining any of them:

- **Line 10**: "suggest a list of **tags**"
- **Line 13**: "develop **meta tags** that can be applied to articles"
- **Line 21**: "tag each article with **labels** that match"
- **Line 23**: "change **tags and labels** on articles"

The First Specification domain model has **both** `tags: ["string"]` and `labels: ["string"]` on the Article entity, implying they are different things, but never defines the distinction.

**Resolution needed:**
- Define "tag" — what is it, who creates it, when is it applied?
- Define "label" — what is it, who creates it, when is it applied?
- Define "meta-tag" — is this a third concept, or is it just a synonym for one of the above?
- Specify the relationship: are tags user-defined categories while labels are AI-generated? Vice versa? Are tags flat strings or hierarchical?

### 2.2 "Working list meta data" (Line 13)

"the app should scan working list **meta data**" — unclear what metadata means here.

**Resolution needed:**
- Does "metadata" mean: (a) RIS fields (title, abstract, authors, year, DOI, journal), (b) AI-generated data from prior steps, (c) user-defined criteria text, or (d) all of the above?
- What specific fields does the AI scan to develop meta-tags?

---

## 3. Priority System Ambiguities

### 3.1 Do exclusion criteria have priorities?

- **Line 6**: "enter a list of inclusion criteria **and priorities**" — suggests priorities apply to inclusion criteria only.
- **Line 7**: "enter a list of exclusion criteria" — no mention of priorities.
- **Line 18**: "an inclusion criterion with same priority is more relevant than an exclusion criterion" — **implies exclusion criteria DO have priorities** (otherwise "same priority" is meaningless).

The First Specification resolves this (Criteria entity has `priority` regardless of type), but the initial requirements contradict themselves.

**Resolution needed:** Explicitly state that both inclusion and exclusion criteria have priorities. Confirm the First Specification's interpretation is correct.

### 3.2 How are priorities assigned?

**Resolution needed:**
- Does the user manually assign a priority to each criterion at creation time?
- Can priorities be changed after criteria are created?
- Can the AI suggest or adjust priorities?

### 3.3 Priority conflict resolution is underspecified

Line 18 says "when there is a disagreement an inclusion criterion with same priority is more relevant". This only covers the tie case.

**Resolution needed:**
- What happens when an article matches multiple inclusion criteria at different priorities AND multiple exclusion criteria at different priorities? Is the decision based on the single highest-priority matched criterion, or is there an aggregate scoring mechanism?
- The First Specification says "A higher priority rule always outweighs a lower priority rule" and "ties favor inclusion." Confirm this is a simple highest-matched-rule comparison, not a weighted sum.

---

## 4. Process Flow Ambiguities

### 4.1 When are tags/meta-tags generated?

The requirements describe two separate AI passes but the ordering is ambiguous:

- **Line 10-13** (before screening): AI suggests tags based on RIS metadata + criteria, then scans working list to develop meta-tags.
- **Lines 16-21** (during screening): AI screens articles, tags them with criteria and labels.

**Resolution needed:**
- Is tag suggestion a one-time batch job before screening starts?
- Are meta-tags generated before screening or after?
- Can the AI update tags after screening based on outcomes?
- What triggers tag generation — user action, automatic on import, automatic after screening?

### 4.2 State machine transitions

The requirements say four lists (imported, working, rejected, included) but the flow description implies:
```
Imported → (deduplication) → Working → (AI review) → Included | Rejected
```

**Resolution needed:**
- Can an article move from Rejected back to Working? (First Specification says yes: "Users can manually override AI decisions and move articles back to the working list at any time.")
- Can an article move directly from Working to Included/Rejected without AI review (manual decision)?
- Can an article move from Included to Rejected directly, or must it go back through Working?
- Can an article move from Included/Rejected back to Imported? (Probably not, but confirm.)
- Draw the complete state transition diagram with all allowed transitions.

### 4.3 What happens to duplicates?

Line 30 says "duplicates are matched and removed."

**Resolution needed:**
- "Removed" from what list? Are they deleted entirely, or moved to a duplicates holding area?
- Does the user confirm deletions, or are they automatic?
- The First Specification says exact matches are auto-merged, fuzzy matches flagged for manual review. Confirm.
- Where is the de-duplicated article's source tracked? (i.e., which of the duplicates was kept and why?)

---

## 5. LLM Integration Ambiguities

### 5.1 Which specific APIs/models?

**Resolution needed:**
- **OpenAI**: Which endpoint? `/v1/chat/completions`? Which models are supported (GPT-4o, GPT-4o-mini, o1, etc.)?
- **Google**: Which API? Gemini? Vertex AI? Which endpoint format?
- **z.ai**: What is this provider? Confirm the correct API format and base URL.
- **Ollama**: Confirm `http://localhost:11434/api/chat` or `/api/generate`?
- **llama.cpp**: Confirm `/v1/chat/completions` OpenAI-compatible endpoint?
- **LM Studio**: Confirm OpenAI-compatible endpoint at `http://localhost:1234/v1/chat/completions`?
- Should the app support custom/unknown OpenAI-compatible endpoints?

### 5.2 What is the exact AI prompt structure?

The requirements say the AI evaluates abstracts against criteria but don't specify the prompt.

**Resolution needed:**
- What is the system prompt template?
- How are criteria injected? As a numbered list? As structured JSON?
- What JSON schema must the LLM return? The First Specification mentions "structured JSON" but doesn't define the schema.
- Does each article get its own API call, or are articles batched into a single prompt?
- If batched: what is the batch size? How is token limit managed?

### 5.3 Batch processing and rate limits

The First Specification mentions "background batches to manage API rate limits" but provides no detail.

**Resolution needed:**
- What is the default batch size?
- How does the app handle rate limit errors (HTTP 429)? Retry with backoff?
- Is there a configurable requests-per-minute limit?
- What happens if the LLM returns malformed/non-JSON output?
- What happens if the LLM connection drops mid-batch? Are partially processed articles marked?

### 5.4 Context window requirement

First Specification says "50,000 tokens or larger."

**Resolution needed:**
- Is the app expected to validate the connected model's context window?
- What happens if the user connects a model with a smaller context window?
- How is the token count estimated before sending a request?

---

## 6. Missing Functional Specifications

### 6.1 Multiple RIS imports

**Resolution needed:**
- Can the user import more than one RIS file into a project?
- If yes, when is deduplication re-run — after each import, or only on demand?
- Can the user add articles to a project that already has screened articles?

### 6.2 Sorting, filtering, and searching

**Resolution needed:**
- How do users navigate large lists (potentially thousands of articles)?
- Is there full-text search across titles and abstracts?
- Can lists be sorted by date, title, relevance score, AI confidence?
- Is there column-based filtering?
- The original Bango platform has a "faceting sidebar" with PICO filtering — is any of this in scope?

### 6.3 Progress tracking

**Resolution needed:**
- During AI screening of potentially thousands of articles, what feedback does the user see?
- Is there a progress bar? Percentage complete? ETA?
- Can the user cancel a running screening job?
- Can the user pause and resume?

### 6.4 Undo / history

**Resolution needed:**
- Is there an undo mechanism for AI decisions?
- Is there an audit log of all state changes?
- Can the user see what the AI changed and selectively revert?

### 6.5 PRISMA diagram specifics

**Resolution needed:**
- Which PRISMA 2020 template? The standard four-phase diagram (Identification → Screening → Eligibility → Included)?
- How is it rendered? SVG, Canvas, HTML/CSS, a charting library?
- What image formats for export? PNG, SVG, PDF?
- The original Bango tracks "how many excluded with specific reasons" — does the PRISMA diagram show breakdowns by exclusion reason?

### 6.6 AI Summary specifics

Line 41: "overall AI summary from abstracts of included articles."

**Resolution needed:**
- How long should the summary be? Word count or token limit?
- Is it a single narrative, or structured (e.g., themes, trends, strengths, weaknesses as separate sections)?
- Is this a one-time generation or can it be regenerated?
- Does the user trigger it manually or is it automatic?

### 6.7 Mobile vs Desktop feature parity

The requirements say "desktop and mobile application" but don't distinguish.

**Resolution needed:**
- Are all features available on mobile?
- Is there a mobile-specific UI paradigm (e.g., swipe-to-include/exclude as in the original Bango)?
- Does mobile support local LLM connections, or only hosted?

### 6.8 Offline capability

The README says "offline-capable" but the initial requirements don't mention it.

**Resolution needed:**
- Does "offline" mean the app works without internet (using local LLMs), or does it mean data is cached for offline viewing?
- If a hosted LLM is configured but there's no internet, what happens to queued screening jobs?

---

## 7. Data Model Ambiguities

### 7.1 RIS field mapping

**Resolution needed:**
- What is the full list of RIS fields the app parses? Title, Abstract, Authors, Year, DOI are mentioned. What about: journal name, volume, issue, pages, keywords, URL, type (journal article, book, conference paper), language, publisher?
- What RIS tags are supported? (e.g., `TI`, `AB`, `AU`, `PY`, `DO`, `T2`, `VL`, `IS`, `SP`, `EP`, `KW`, `UR`, `TY`, `LA`, `PB`)
- What happens with unsupported or custom RIS tags? Are they preserved in `risData` or dropped?

### 7.2 Deduplication algorithm

**Resolution needed:**
- Exact match: what fields must match? (title + year + first author? title + DOI? DOI alone?)
- Fuzzy match: what similarity threshold? What algorithm (Levenshtein, Jaro-Winkler, cosine similarity on embeddings)?
- Are there secondary matching strategies (e.g., DOI match first, then title+year fallback)?
- How are conflicting metadata fields resolved when merging duplicates (e.g., one copy has an abstract, another doesn't)?

### 7.3 Research aims format

**Resolution needed:**
- Is "research aims" a single free-text block, or a list of discrete aim entries (like criteria)?
- The domain model shows `researchAims: "string"` — single string. Confirm.
- How does the AI use research aims vs criteria during screening? Are aims part of the system prompt, evaluated as separate criteria, or used differently?

### 7.4 Export format for project backup

Line 45: "any compact data format including JSON." First Specification: "JSON format."

**Resolution needed:**
- Confirm: JSON only, or should other formats (e.g., SQLite dump, ZIP archive) be supported?
- What is the JSON schema for project export?
- Is the export a single file or a directory of files?
- Are API keys included in the export? (Security concern — the First Specification says "LLM configurations" are included.)
- Is the exported file encrypted or password-protected?

---

## 8. Non-Functional Gaps

### 8.1 Performance requirements

**Resolution needed:**
- What is the maximum supported RIS file size?
- What is the maximum number of articles per project?
- What is the acceptable UI response time for list operations (filtering, sorting, searching)?
- The First Specification mentions a "warning at 80% of SQLite limits" — what are those limits and what happens when they're reached?

### 8.2 Data validation

**Resolution needed:**
- What constitutes a "valid" RIS file?
- What happens when an article has no abstract? Is it screened with title-only, skipped, or flagged?
- What happens when an article has no title?
- Minimum required fields for an article to be processable?

### 8.3 Security

**Resolution needed:**
- How are API keys encrypted? What encryption algorithm?
- Where is the encryption key stored?
- Is the local SQLite database encrypted?
- Are there any access controls on the app itself (password, biometric)?

---

## Summary of Critical Decisions Needed

These are the items that will directly impact architecture and cannot be deferred:

1. **Define Tags vs Labels vs Meta-tags** — affects data model and UI
2. **Complete state transition diagram** — affects core logic and database schema
3. **Confirm exclusion criteria have priorities** — affects AI screening logic
4. **Define LLM prompt structure and JSON response schema** — affects Rust backend and AI integration
5. **Specify deduplication algorithm** — affects Rust backend
6. **Define PRISMA rendering approach** — affects frontend tech choice
7. **Clarify mobile vs desktop scope** — affects Tauri configuration and UI architecture
8. **Complete line 24** (the cut-off sentence) — unknown scope
