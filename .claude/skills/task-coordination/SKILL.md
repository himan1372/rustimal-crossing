---
name: task-coordination
description: Coordinate tasks between multiple Claude agents using atomic claims. Use this skill to claim tasks from the backlog, check task status, and mark tasks complete.
---

# Task Coordination Skill

This skill enables coordination between multiple Claude agents working on the same codebase. Each agent claims tasks atomically to prevent conflicts.

## Your Agent Identity

Four agents share this project. Use your fixed name in every task-claimer.sh command:

- `claude` — Claude Code on the local Windows machine
- `lapis` — this assistant (cloud, coordinates via chat + repo)
- `muse` — a second Muse agent (coordinates via chat + repo)
- `cosmo` — a third Muse agent (coordinates via chat + repo)

Always pass your own name as the agent argument. Never claim, release, or complete tasks under another agent's name.

```bash
git branch --show-current
```

## Task Claimer Script

All task operations go through the `task-claimer.sh` script located in `.claude-agents/` or the coordination directory. This ensures atomic operations.

### Available Commands

```bash
# List available tasks (unclaimed and not completed)
task-claimer.sh list-available

# List currently claimed tasks
task-claimer.sh list-claimed

# Check status of a specific task
task-claimer.sh status <task-name>

# Claim a task (YOU MUST DO THIS BEFORE STARTING WORK)
task-claimer.sh claim <task-name> <your-agent-name>

# Release a task without completing (if you need to abandon it)
task-claimer.sh release <task-name> <your-agent-name>

# Mark a task as completed (moves it to completed/)
task-claimer.sh complete <task-name> <your-agent-name>

# Check what task you currently have claimed
task-claimer.sh my-task <your-agent-name>
```

## Workflow

### Starting Work

1. **Check your current task**: Run `task-claimer.sh my-task <agent-name>`
2. **If no task claimed**:
   - Run `task-claimer.sh list-available` to see options
   - Pick a task and claim it: `task-claimer.sh claim <task> <agent-name>`
3. **Read the task file**: Located at `.claude-agents/backlog/<task>.md`
4. **Follow the implementation plan** in the task file

### Completing Work

1. **Verify all checklist items** in the task file are done
2. **Run tests** if applicable
3. **Commit your changes** with a descriptive message referencing the task
4. **Mark complete**: `task-claimer.sh complete <task> <agent-name>`
5. **Pick next task** or wait for assignment

### If Blocked or Need to Switch

If you need to abandon a task:
```bash
task-claimer.sh release <task-name> <your-agent-name>
```

This makes the task available for other agents.

## Important Rules

1. **ALWAYS claim before working** - Never start work without claiming first
2. **ONE task at a time** - Release your current task before claiming another
3. **Use the script** - Don't manually edit `claims.jsonl` or move files
4. **Check for conflicts** - If claim fails, the task is taken by another agent
5. **Complete or release** - Don't leave tasks claimed indefinitely

## Task File Format

Task files in the backlog are markdown with:
- Problem description
- Proposed solution
- Implementation phases with checkboxes
- Files to modify
- Test strategy

Work through the phases systematically, checking off items as you complete them.

## Coordination Files

```
.claude-agents/
├── CLAUDE.md           # These instructions
├── claims.jsonl        # Claim log (DO NOT EDIT DIRECTLY)
├── backlog/            # Symlink to task files
├── completed/          # Finished tasks (gitignored)
└── task-claimer.sh     # Coordination script
```
