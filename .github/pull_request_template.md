## Description

<!-- Explain what changed and why. Include context from related issues if applicable. -->

Closes #(issue)

## Type of Change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Refactor (code change that improves structure without changing behavior)
- [ ] Test addition or improvement
- [ ] Documentation update
- [ ] Breaking change (fix or feature that changes existing behavior)

## Verification

<!-- Run the verification pipeline before marking ready. -->

- [ ] `bun run build` passes
- [ ] `bun run --filter photrez-desktop test --run` passes (full suite)
- [ ] `cargo test -p photrez-core` passes
- [ ] New behavior has **wiring tests** (user-event simulation) where applicable
- [ ] New behavior has **state contract tests** where applicable
- [ ] UI changes preserve existing layout (tabs, dock hierarchy, panel ownership)
- [ ] Relevant docs updated (ARCHITECTURE.md, FEATURES.md, keyboard shortcuts, etc.)

## Checklist

- [ ] My code follows the project's TypeScript strict style and SolidJS conventions
- [ ] I have added tests that prove my fix is effective or that my feature works
- [ ] I have not introduced new dependencies without explaining why the existing stack is insufficient
- [ ] I have kept unrelated refactors out of this PR

## Screenshots (if applicable)

<!-- Add screenshots to help reviewers understand UI changes. -->
