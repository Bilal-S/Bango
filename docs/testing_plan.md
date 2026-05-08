# Bango v3 Testing Plan

This document defines the comprehensive testing strategy for the Bango application, covering both the Rust backend and Vue frontend. The plan ensures alignment with the Bango v3 technical specifications and development rules.

## 1. Objectives

- **Verify Core Logic**: Ensure RIS parsing, deduplication, and screening resolution follow specified rules.
- **Data Integrity**: Guarantee that database operations and project export/import preserve all metadata.
- **UI Consistency**: Validate that state management and derived UI data (like PRISMA counts) are accurate.
- **Regression Testing**: Maintain a suite that can be run with `cargo test` and `vitest` to prevent future breaks.

## 2. Backend (Rust) Testing Plan

### 2.1 Domain Logic (Unit & Integration)
- **RIS Parsing**: 
    - Verify support for all tags in Section 4.1.
    - Test concatenated tag handling (e.g., `AD  - ...C3  - ...`).
    - Validate fallback logic (N2 for Abstract, etc.).
- **Deduplication**:
    - Validate all 4 matching strategies (Section 5.1).
    - Test title normalization and short-title guard (Section 5.2).
- **Screening Resolution**:
    - **Determinism**: Test `resolve_decision` with various priority combinations.
    - **Tie-breaking**: Verify that equal priority matches result in "include".
    - **Default Exclusion**: Verify that no matches result in "exclude".

### 2.2 System Operations
- **Encryption**: Test AES-256-GCM encryption for local storage and export.
- **Export/Import**: 
    - Verify `ExportMetadata` versioning.
    - Test password-protected project import/export.
    - Validate RIS export formatting, specifically the `C1` tag for labels.

## 3. Frontend (Vue) Testing Plan

### 3.1 State Management (Pinia Stores)
- **Articles Store**: 
    - Test asynchronous fetching with mocked Tauri commands.
    - Verify computed counts (`byStatus`) match current state.
- **Criteria Store**: 
    - Test CRUD operations for research aims and criteria.
    - Verify priority level assignments.

### 3.2 UI Logic & Utilities
- **PRISMA Mapping**: 
    - Test the logic that maps article statuses to PRISMA diagram boxes (Section 12.1).
- **Formatters**: 
    - Test date and text formatters used throughout the UI.

## 4. Test Execution Guide

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

## 5. Sample Data Requirements
To support this plan, the following test fixtures are required in `tests/assets`:
- `valid_multi_record.ris`: Standard compliant RIS.
- `missing_fields.ris`: RIS with missing TI/AB/AU for validation testing.
- `concatenated_tags.ris`: RIS with non-standard formatting.
- `duplicates.ris`: Set of articles with varying degrees of similarity for dedup testing.
