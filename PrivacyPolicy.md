# Privacy Policy for Bango

**Last updated: May 2026**

## Overview

Bango is a privacy-first desktop application for systematic literature review. It is designed as a single-user, self-contained application where all data remains on your device.

## Data Collection

We do not collect any personal data, usage data, or any other kind of data from you.

- No personally identifiable information is collected
- No usage statistics or analytics are gathered
- No crash reports are sent to any server
- No device or hardware information is transmitted

## Data Sharing

We do not share any data with any third party, because we do not collect any data in the first place.

- No data is sold or distributed
- No advertising identifiers are used
- No user accounts or profiles exist
- No cloud services or external servers are involved

## Local Storage

All your data is stored locally on your device in a SQLite database that Bango manages. This includes:

- Imported research articles
- Screening criteria and research aims
- Screening decisions and audit trails
- Tags, labels, and notes
- Duplicate detection results
- PRISMA flow diagrams

You have full control over this data at all times. You can export or delete it at any time using the built-in export and management features.

## AI Provider Communications

Bango supports AI-assisted screening and summarization. When you choose to use these features:

- **You choose your AI provider.** Bango supports multiple providers including OpenAI, Anthropic, Google, Mistral, and any OpenAI-compatible local provider such as Ollama, llama.cpp, or LM Studio.
- **You choose your model.** You select which specific model to use for each task.
- **You configure the endpoint.** You provide the API endpoint URL, which can be a local server address (e.g., `http://localhost:11434`) or a remote provider URL.
- **Only article data is sent.** When AI screening is active, article text (title, abstract) is sent to the provider you configured for the purpose of evaluating inclusion/exclusion criteria. No other data is transmitted.
- **Bango never sees this data.** Communication happens directly between your device and the AI provider you selected. The Bango development team has no access to your articles, API keys, or screening results.
- **Local providers are fully supported.** If you use a local AI provider (such as Ollama), no data leaves your device at all.

API keys provided for remote providers are stored locally in an encrypted format using AES-256-GCM encryption and are never transmitted anywhere except to the configured provider endpoint for authentication.

## Network Usage

Bango makes network requests only in the following user-initiated scenarios:

1. **AI screening** - Sending article text to your configured AI provider for screening evaluation
2. **AI summarization** - Sending article text to your configured AI provider for summary generation
3. **AI label/tag suggestions** - Sending criteria information to your configured AI provider for label recommendations
4. **Model listing** - Fetching available models from your configured AI provider endpoint

All of these are optional and only occur when you explicitly configure an AI provider and trigger the relevant action.

Bango does **not** make any of the following network requests:

- Telemetry or analytics
- Crash reporting
- Phone-home or update checking
- Advertising or tracking
- Any background network activity

## Third-Party Services

Bango does not integrate with any third-party services. There are:

- No user accounts or sign-in systems
- No cloud storage or synchronization
- No social media integrations
- No payment processing within the app
- No advertising networks

## Children's Privacy

Bango is a research tool not directed at children under the age of 13. We do not knowingly collect information from children.

## Security

Bango takes reasonable measures to protect your data:

- All data is stored locally on your device
- API keys are encrypted at rest using AES-256-GCM with a key derived via PBKDF2
- No data is transmitted over the network except to AI providers you explicitly configure
- The application does not require any special network permissions beyond standard HTTP/HTTPS access

## Changes to This Privacy Policy

We may update this privacy policy from time to time. Any changes will be reflected in the "Last updated" date at the top of this document and will be committed to the project repository.

## Contact

If you have questions about this privacy policy, please open an issue on the [Bango GitHub repository](https://github.com/Bilal-S/Bango).