---
name: plan
description: "Create a detailed implementation plan for a feature or change. Researches the codebase, identifies affected crates, and produces a phased plan with dependencies and risks."
disable-model-invocation: true
context: fork
agent: planner
argument-hint: "[feature or change description]"
---

Create a detailed implementation plan for the following request:

$ARGUMENTS

## Instructions

1. **Research the codebase** thoroughly before planning:
   - Search for related code, types, traits, and modules
   - Identify which crates are affected
   - Check existing patterns and conventions
   - Look at test structure and coverage

2. **Analyze architecture impact**:
   - Which of the 3 threads (Main/IO/Renderer) are affected?
   - Are new crossbeam channels needed?
   - Does shared state (Arc<Mutex<TerminalState>>) need changes?
   - Are crate dependency boundaries respected?

3. **Produce the plan** using this structure:

```markdown
# Implementation Plan: [Feature Name]

## Overview
[2-3 sentence summary]

## Affected Crates
- [ ] minal-core: [what changes]
- [ ] minal-renderer: [what changes]
- [ ] minal-ai: [what changes]
- [ ] minal-config: [what changes]
- [ ] src/ (app): [what changes]

## Implementation Steps

### Phase 1: [Name] (independently deliverable)
1. **[Step]** (Crate: X, File: path)
   - Action: ...
   - Why: ...
   - Dependencies: None / Step N
   - Risk: Low/Medium/High

### Phase 2: [Name]
...

## Thread Model Impact
...

## Testing Strategy
...

## Risks & Mitigations
...
```

4. **Ensure each phase is independently mergeable** — avoid plans that only work when all phases are complete.
