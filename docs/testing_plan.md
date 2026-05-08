# Bango v3 Testing Plan

This document defines the comprehensive testing strategy for the Bango application, covering both the Rust backend and Vue frontend. The plan ensures alignment with the Bango v3 technical specifications and development rules.

## 1. Objectives

- **Verify Core Logic**: Ensure RIS parsing, deduplication, and screening resolution follow specified rules.
- **Data Integrity**: Guarantee that database operations and project export/import preserve all metadata.
- **System Resilience**: Validate background job handling, rate-limiting, and error recovery.
- **Performance & Scale**: Ensure UI responsiveness and system stability at the 10,000 article limit.
- **UI Consistency**: Validate that state management and derived UI data (like PRISMA counts) are accurate.

## 2. Backend (Rust) Testing Plan

### 2.1 Domain Logic (Unit & Integration)
- **RIS Parsing**: 
    - Verify support for all tags in Section 4.1.
    - Test concatenated tag handling (e.g., `AD  - ...C3  - ...`).
    - Validate fallback logic (N2 for Abstract, etc.).
- **Deduplication**:
    - Validate all 4 matching strategies (Section 5.1).
    - Test title normalization and short-title guard (Section 5.2).
    - **Multi-Import**: Verify that deduplication re-runs across the entire Imported list after subsequent imports (Section 4.4).
    - **Manual Resolution**: Test marking an article with `duplicateOf` after a fuzzy match comparison.
- **Screening Resolution**:
    - **Determinism**: Test `resolve_decision` with various priority combinations.
    - **Tie-breaking**: Verify that equal priority matches result in "include".
    - **Default Exclusion**: Verify that no matches result in "exclude".
    - **Retry Logic**: Verify that individual retries clear the `screeningError` flag (Section 7.4).

### 2.2 System Operations & Resilience
- **Screening Job Engine**:
    - **Concurrency**: Verify the 3-request concurrent limit.
    - **Rate Limiting**: Test exponential backoff (1s, 2s, 4s) on HTTP 429 errors.
    - **Pause/Resume**: Verify stopping after current article and skipping processed records on resume.
- **AI Summary Batching**:
    - Test splitting articles into sub-batches when combined text exceeds context window (Section 11).
    - Verify synthesis logic for merging batch summaries.
- **LLM Configuration**:
    - **Connection Testing**: Verify "Test Connection" logic for various providers (OpenAI, Ollama, etc.).
- **Encryption & Import/Export**: 
    - Verify `ExportMetadata` versioning.
    - **Failure Path**: Test importing a project with an incorrect password (ensure data loads without API keys).
    - **RIS Metadata**: Validate export of user notes using the `NO` tag and labels using the `C1` tag (Section 15.1).

### 2.3 Data Consistency
- **Audit Trail**: 
    - Verify that **every** manual status change, tag update, or screening result creates a valid `AuditEntry`.

## 3. Frontend (Vue) Testing Plan

### 3.1 State Management (Pinia Stores)
- **Articles Store**: 
    - Test asynchronous fetching with mocked Tauri commands.
    - **Filter Matrix**: Verify complex multi-select "AND" logic (Tags + Labels + Status).
    - **Sorting**: Test logic for multi-column sorting (Title A-Z, Year newest first, Confidence highest first).
    - Verify computed counts (`byStatus`) match current state.
- **Criteria Store**: 
    - Test CRUD operations for research aims and criteria.
    - Verify priority level assignments.

### 3.2 UI Logic & Utilities
- **PRISMA Mapping**: 
    - Test the logic mapping article statuses to the 4-phase PRISMA flow (Identification -> Screening -> Eligibility -> Included).
- **Token Estimation**:
    - Verify UI warning triggers when estimated tokens exceed 80% of context window (Section 9.6).
- **Formatters**: 
    - Test date and text formatters used throughout the UI.

## 4. Performance & Scale Targets

- **Capacity**: Test system stability with 10,000 imported articles.
- **Latency**: Verify that list operations (search/sort/filter) complete in **< 200ms** even at scale.
- **Cold Start**: Ensure app initialization completes in **< 3s**.

## 5. Test Execution Guide

### Running Backend Tests
```bash
cd src-tauri
cargo test
```

### Running Frontend Tests
```bash
npm run test
```

### Full Quality Check
```bash
npm run check:all
```

## 6. Sample Data Requirements
To support this plan, the following test fixtures are required in `tests/assets`:
- `valid_multi_record.ris`: Standard compliant RIS.
- `missing_fields.ris`: RIS with missing TI/AB/AU for validation testing.
- `concatenated_tags.ris`: RIS with non-standard formatting.
- `duplicates.ris`: Set of articles with varying degrees of similarity for dedup testing.
- `large_dataset.ris`: A set of 5,000+ records for performance benchmarking.
