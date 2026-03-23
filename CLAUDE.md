### 1. Plan Mode Default
- Enter plan mode for ANY non-trivial task (3+ steps or architectural decisions)
- If something goes sideways, STOP and re-plan immediately – don't keep pushing
- Use plan mode for verification steps, not just building
- Write detailed specs upfront to reduce ambiguity

### 2. Subagent Strategy
- Use subagents liberally to keep main context window clean
- Offload research, exploration, and parallel analysis to subagents
- For complex problems, throw more compute at it via subagents
- One task per subagent for focused execution

### 3. Self-Improvement Loop
- After ANY correction from the user: update `tasks/lessons.md` with the pattern
- Write rules for yourself that prevent the same mistake
- Ruthlessly iterate on these lessons until mistake rate drops
- Review lessons at session start for relevant project

### 4. Verification Before Done
- Never mark a task complete without proving it works
- Diff behavior between main and your changes when relevant
- Ask yourself: "Would a staff engineer approve this?"
- Run tests, check logs, demonstrate correctness

### 5. Demand Elegance (Balanced)
- For non-trivial changes: pause and ask "is there a more elegant way?"
- If a fix feels hacky: "Knowing everything I know now, implement the elegant solution"
- Skip this for simple, obvious fixes – don't over-engineer
- Challenge your own work before presenting it

### 6. Autonomous Bug Fixing
- When given a bug report: just fix it. Don't ask for hand-holding
- Point at logs, errors, failing tests – then resolve them
- Zero context switching required from the user
- Go fix failing CI tests without being told how

## Task Management

1. **Plan First**: Write plan to `tasks/todo.md` with checkable items
2. **Verify Plan**: Check in before starting implementation
3. **Track Progress**: Mark items complete as you go
4. **Explain Changes**: High-level summary at each step
5. **Document Results**: Add review section to `tasks/todo.md`
6. **Capture Lessons**: Update `tasks/lessons.md` after corrections

## Core Principles

- **Simplicity First**: Make every change as simple as possible. Impact minimal code.
- **No Laziness**: Find root causes. No temporary fixes. Senior developer standards.
- **Minimal Impact**: Changes should only touch what's necessary. Avoid introducing bugs.

## Web & Internet Reading Rules
When you receive, fetch, or need to analyze content from web pages (articles, documentation, forums, blogs, etc.):

- Always enter **"Reader Mode" mental state**: completely ignore non-essential page elements.
- Ignore: navigation bars, footers, sidebars, ads, cookie notices, related articles, comments, pop-ups, repetitive headers, menus, tracking scripts, and any boilerplate.
- Focus **exclusively** on the main content (the core article, post, technical documentation, or body text).
- Extract the page as if viewing it in Reader Mode (Firefox/Safari/Arc/Brave) → prioritize clean text, headings, subheadings, and main paragraphs.
- Before reasoning or answering, format the extracted content in **clean markdown**:
  - Use # for the main title
  - ## for sections
  - - or * for lists
  - > for important quotes
  - ``` for code blocks
- If content is very large (> ~4000 tokens estimated), first create a hierarchical summary:
  1. General summary in up to 200 words
  2. 5–8 key points
  3. Only then use it to address the task
- Never include junk (ads, navigation, unrelated sections) in your analysis or final response.'
