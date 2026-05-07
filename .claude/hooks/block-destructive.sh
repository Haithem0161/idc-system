#!/bin/bash
# PreToolUse hook: Block destructive Docker, git, and filesystem commands.
# Returns JSON with permissionDecision "deny" to block the tool call.

COMMAND=$(jq -r '.tool_input.command // empty')

if [ -z "$COMMAND" ]; then
  exit 0
fi

# Block destructive Docker commands.
if echo "$COMMAND" | grep -qE '(docker\s+rm\b|docker\s+compose\s+rm|docker\s+system\s+prune|docker\s+container\s+prune|docker\s+volume\s+prune|docker\s+image\s+prune)'; then
  jq -n '{
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: "BLOCKED: Destructive Docker command detected. This project forbids: docker rm, docker compose rm, docker system prune, docker container prune, docker volume prune, docker image prune."
    }
  }'
  exit 0
fi

# Block destructive git commands unless explicitly authorized.
if echo "$COMMAND" | grep -qE '(git\s+push\s+(-f|--force)|git\s+reset\s+--hard|git\s+clean\s+-fd|git\s+branch\s+-D|git\s+filter-branch|git\s+update-ref\s+-d)'; then
  jq -n '{
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: "BLOCKED: Destructive git command detected (force push, hard reset, clean -fd, branch -D, filter-branch, update-ref -d). Ask the user before retrying with explicit authorization."
    }
  }'
  exit 0
fi

# Block --no-verify / --no-gpg-sign on commits.
if echo "$COMMAND" | grep -qE 'git\s+commit\b.*(--no-verify|--no-gpg-sign|-c\s+commit\.gpgsign=false)'; then
  jq -n '{
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: "BLOCKED: Skipping commit hooks or signing is forbidden. Fix the underlying issue instead."
    }
  }'
  exit 0
fi

# Block obviously destructive filesystem operations.
if echo "$COMMAND" | grep -qE '(\brm\s+-rf\s+/(\s|$)|\brm\s+-rf\s+~/?\s|\brm\s+-rf\s+\.\s|sudo\s+rm\s+-rf)'; then
  jq -n '{
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: "BLOCKED: Dangerous rm -rf path. If you really need this, ask the user with full context."
    }
  }'
  exit 0
fi

# Block manual edits to package.json / Cargo.toml dependency sections via heredoc/echo.
# (Simple guard -- editors are expected via Edit tool, not via shell.)
if echo "$COMMAND" | grep -qE '>\s*package\.json|>\s*Cargo\.toml'; then
  jq -n '{
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: "BLOCKED: Do not overwrite package.json or Cargo.toml from the shell. Use pnpm add / cargo add for dependencies, or the Edit tool for surgical changes to other sections."
    }
  }'
  exit 0
fi

exit 0
