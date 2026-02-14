# Workspace Agent Rules

## Skill-First Policy for Word Tasks

For any request to create, edit, or export Microsoft Word files (`.doc` / `.docx`), follow this order strictly:

1. Run `find-skills` first to discover Word-capable skills.
2. If a suitable skill exists, run `skill-installer` to install it.
3. Use the installed skill to complete the task.
4. Only if no suitable skill is found, or installation/use is blocked, fall back to a direct code implementation (for example Python).

## Fallback Requirement

When using fallback code instead of a skill, state the concrete reason briefly (for example: "no matching skill found" or "skill installation failed due to source access").
