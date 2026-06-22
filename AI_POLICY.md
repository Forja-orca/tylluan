# AI Contribution Policy

## Purpose

Tylluan is built by humans and AI agents working together. This policy governs how AI-generated contributions are handled to maintain quality, security, and transparency.

## Disclosure Requirements

All contributions must disclose AI involvement:

- **AI-generated:** Code, documentation, or other content primarily written by an AI agent
- **AI-assisted:** Content where an AI agent helped with research, drafting, or review but a human made the final decisions
- **Human-written:** No AI involvement

Use the following format in pull request descriptions:

```
AI Disclosure: [AI-generated | AI-assisted | Human-written]
Agent: [agent name/model, if applicable]
Human reviewer: [GitHub username]
```

## Rules for AI Agents

If you are an AI agent contributing to Tylluan:

1. **Do not open issues or PRs autonomously.** A human must review and submit.
2. **Do not fabricate test results.** If tests fail, report the failure honestly.
3. **Do not include personal data** from your training, context, or operator's environment.
4. **Do not claim functionality that does not exist.** If you are unsure whether code works, say so.
5. **Do not modify security configuration** (auth, token handling, network binding) without explicit human approval.
6. **Do not bypass CI checks.** If your code fails linting or tests, fix it — do not skip verification.

## Rules for Human Operators

If you are a human submitting AI-generated contributions:

1. **Review all AI-generated code** before submitting. You are accountable for its quality and safety.
2. **Run the test suite** (`cargo test -p tylluan-kernel --lib`) before submitting.
3. **Check for secrets and personal data** that the AI agent may have inadvertently included.
4. **Do not submit AI-generated code you do not understand.** If you cannot explain what the code does, do not merge it.

## Automated Contributions

Automated systems (bots, CI/CD pipelines, scheduled agents) that submit contributions must:

- Be clearly identified as automated in their Git author metadata
- Not have write access to protected branches
- Have their output reviewed by a human before merge

## Moderation

Maintainers may reject any contribution that:

- Fails to disclose AI involvement
- Contains fabricated or misleading information
- Introduces security vulnerabilities
- Includes personal data, secrets, or local paths
- Does not follow the project's code style and architecture rules

## Precedent

This policy is informed by similar policies in [Letta](https://github.com/letta-ai/letta) (AI_POLICY.md) and the emerging AGENTS.md standard adopted by several major open-source AI projects.
