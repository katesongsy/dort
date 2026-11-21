# Working on Dort

- Be concise in all UI text, documentation and code comments. Use caveman style: short, direct sentences. State what something is or does. Avoid defensive phrasing and unnecessary contrasts such as “this doesn't do X” or “this isn't Y.” Mention limitations, expectations or rationale only when needed for correct use or an informed decision.
- Prefer straightforward defaults, clear actions and short explanations. Keep routine workflows simple while preserving necessary safeguards.
- Avoid internal jargon in everything users see: UI text, errors, CLI output, reports and `docs/user`. Name the concrete object, problem and next action.
- Keep internal documents minimal, concise and high-level. Do not restate implemented behavior from the code. Keep details only when they are not evident in the code and need human attention.
- Write `docs/user` for end users, not developers: user-friendly language, no internal implementation details.
- Put design and implementation contracts in ignored `specs/` only. They are obsolete once implemented; do not maintain them as documentation.

## Code principles

* Architecture: Separate business logic from UI, persistence and external systems. Prefer high cohesion, low coupling and consistent dependency direction.
* Readability: Use descriptive names, focused functions, explicit error handling and simple control flow.
* Maintainability: Reuse existing patterns, avoid premature abstraction and keep changes localized.
* Verification: Run formatting, linting, type checks and tests. Review the final diff for design problems.
