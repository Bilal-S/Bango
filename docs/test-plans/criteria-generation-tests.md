# Criteria Commands - Test Inventory

Binding test inventory for the criteria harmonization fix (AI generation plus
holistic ruleset review). Enforced by `scripts/check-test-inventory.sh` via
`npm run check:all`.

The `file::function` rows below are machine-checked: the script greps each
named test file for the listed function name. Any missing test blocks the PR.

## Pure helper (`commands::criteria::build_criteria_generation_prompt`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/criteria_generation_test.rs::build_criteria_prompt_includes_opposite_criteria` | When opposite-type criteria exist, their text appears in the prompt so the LLM can avoid mirroring them. |
| `src-tauri/tests/criteria_generation_test.rs::build_criteria_prompt_harmonization_guidance_present` | The prompt carries the division-of-labor and "do not negate" guidance. |
| `src-tauri/tests/criteria_generation_test.rs::build_criteria_prompt_aims_only_degrades_gracefully` | An empty opposite-criteria list renders a placeholder instead of crashing (first generation of either type). |
| `src-tauri/tests/criteria_generation_test.rs::build_criteria_prompt_inclusion_and_exclusion_branches` | Both `criterion_type` values produce valid, distinct prompts with correctly flipped opposite-type labels. |

## Pure helper (`commands::criteria::build_check_rules_prompt`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/criteria_generation_test.rs::build_check_rules_prompt_flags_negation_guidance` | The holistic review prompt flags exclusion criteria that merely negate an inclusion and recommends deleting them. |
| `src-tauri/tests/criteria_generation_test.rs::build_check_rules_prompt_renders_custom_logic` | Custom screening instructions render when present and fall back to a placeholder when absent. |
