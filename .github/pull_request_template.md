## What this changes

<!-- What does this pull request change, and why? Link the issue it closes, if any. -->

## How it was tested

<!-- Tests you added or ran, and what you checked in the app. -->

## Checklist

- [ ] `cargo fmt --check`, `cargo clippy --release -- -D warnings` and `cargo test --release -- --test-threads=1` pass in `dashboard/`
- [ ] If the UI changed, the snapshots were re-captured with `UPDATE_SNAPSHOTS=1` and committed
- [ ] `dotnet test sensord/tests/Sensord.Tests.csproj` passes, and new behavior has a test
- [ ] The CHANGELOG.md *Unreleased* section has a line for anything users notice
