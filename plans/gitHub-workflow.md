Use these project specs in `docs/superpowers/specs/bango-v3-spec.md`
Use development rules  in `CLAUDE.md`

Task: Create a GitHub Actions workflow for a Tauri 2.x project that synchronizes versions and builds multi-platform releases.

Requirements:

Trigger: Set the workflow to run only when a tag matching v* is pushed.

Version Sync Job:

Check out the repository with write permissions.

Extract the version number from the git tag.

Run npx tauri version <version> to update package.json, tauri.conf.json, and Cargo.toml.

Commit and push these changes back to the main branch.

Build Job:

Depend on the completion of the version sync job.

Use a build matrix including windows-latest, ubuntu-latest, and macos-latest.

Install necessary Linux system dependencies for WebKit and GTK.

Configure the macos-latest runner to build both the desktop app and the iOS/iPad target.

Use tauri-apps/tauri-action to compile the code and create a GitHub Release.

Reference GitHub Secrets for Apple and Windows code signing certificates.