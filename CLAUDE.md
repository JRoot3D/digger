# Digger (Rust) — instructions for Claude Code

Read `docs/porting-plan.md` and `openspec/config.yaml` before any work.

## Who does what

- **Planning runs on Fable.** Proposals, designs, spec deltas, task lists and
  any revision of them (`/opsx:propose`, `/opsx:update`, `/opsx:explore`) are
  done by the main session on the Fable model.
- **Implementation is delegated to Opus.** When applying a change
  (`/opsx:apply`), the main session does not write the code itself. It hands
  each task group to a subagent started with `model: "opus"`, gives it the
  change's proposal, design, specs and the task group, reviews what comes
  back against the specs, ticks the tasks, and commits.

## Hard rules

- No C: nothing from upstream is linked, vendored, called through FFI, run or
  mechanically translated. No SDL. Upstream data tables and recordings may be
  read as data.
- Implement from the specs and `docs/research/`. When a question or spec
  conflict remains, re-check the provided sources (upstream source and
  recordings, digger.org, portmaster.games) to confirm the behaviour, then write
  the code independently. Never copy or translate C.
- One Conventional Commit per completed task group at minimum.
