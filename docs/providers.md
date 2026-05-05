Here's the complete AI settings summary for the ShortSizzle project, sourced from `useSettings.ts` and the provider files in `app/composables/llm/providers/`:

---

## Cloud Providers (5)

| # | Provider | Default Model | Default Base URL |
|---|----------|--------------|-----------------|
| 1 | **OpenAI** | `gpt-5-nano` | `https://api.openai.com/v1` |
| 2 | **Anthropic** | `claude-haiku-4-5-20251001` | `https://api.anthropic.com/v1/messages` |
| 3 | **Google Gemini** | `gemini-3-flash` | `https://generativelanguage.googleapis.com/v1beta` |
| 4 | **Mistral AI** | `mistral-nemo` | `https://api.mistral.ai/v1` |
| 5 | **Z.AI** | `GLM-4-Flash` | `https://api.z.ai/api/paas/v4` |

### All Available Models per Provider

**OpenAI:** `gpt-5.4`, `gpt-5.4-pro`, `gpt-5.4-mini`, `gpt-5.4-nano`, `gpt-5-mini`, `gpt-5-nano`, `gpt-5`, `gpt-4.1`, `gpt-realtime`, `gpt-5-codex`, `gpt-5.3-codex`, `gpt-5.2-codex`, `gpt-5.1-codex`, `gpt-5.1-codex-max`

**Anthropic:** `claude-opus-4-6`, `claude-opus-4-5`, `claude-opus-4-1`, `claude-opus-4`, `claude-sonnet-4-6`, `claude-sonnet-4-5`, `claude-sonnet-4`, `claude-3-7-sonnet-20250219`, `claude-3-5-sonnet-20241022`, `claude-haiku-4-5-20251001`, `claude-3-5-haiku-20241022`, `claude-3-haiku-20240307`

**Google Gemini:** `gemini-2.0-flash-001`, `gemini-2.0-flash-lite-001`, `gemini-2.5-flash`, `gemini-2.5-flash-lite`, `gemini-2.5-flash-image`, `gemini-2.5-flash-live`, `gemini-2.5-pro`, `gemini-2.5-pro-tts`, `gemini-2.5-flash-tts`, `gemini-2.5-computer-use`, `gemini-2.5-deep-think`, `gemini-3-flash`, `gemini-3-pro`, `gemini-3-pro-image`, `gemini-3.1-flash`, `gemini-3.1-flash-lite`, `gemini-3.1-flash-live`, `gemini-3.1-flash-image`, `gemini-3.1-pro`

**Mistral AI:** `mistral-large-2407`, `mistral-small-2402`, `mistral-nemo`, `ministral-8b-2410`, `ministral-3b-2410`, `codestral-2501`

**Z.AI:** `glm-4.5`, `glm-4.5-air`, `glm-4.6`, `glm-4.7`, `glm-5`, `glm-5-turbo`, `glm-5.1`

---


---

### Key Notes
- **Default behavior** when users selects a provider we default the endpoint, but it can be changed
    - the models input become selectable including an `other` option that user can enter directly
- OpenAI, Mistral, and Z.AI use the **OpenAI-compatible** `/chat/completions` endpoint pattern.
- Anthropic uses its own **Messages API** format (`x-api-key` header, `anthropic-version` header).
- Gemini uses Google's **`generateContent`** REST endpoint with the API key as a query parameter.
