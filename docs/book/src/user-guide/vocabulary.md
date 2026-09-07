# Vocabulary

Each project has a **VocabularyMap**: 16 configurable label keys that rename terms throughout
the UI. Two projects can use completely different language while running on the same underlying
system.

---

## The 16 Keys

| Key | Default | Construction example | Homework example |
|---|---|---|---|
| `epic` | Epic | Building | Course |
| `feature` | Feature | Section | Module |
| `task` | Task | Work Order | Assignment |
| `subtask` | Subtask | Activity | Question |
| `bug` | Bug | Defect | Correction |
| `requirement` | Requirement | Specification | Rubric Item |
| `sprint` | Sprint | Phase | Week |
| `backlog` | Backlog | Pending Work | Upcoming |
| `board` | Board | Project Board | Planner |
| `blocker` | Blocker | Hold | Dependency |
| `story_points` | Story Points | Effort Hours | Effort |
| `assignee` | Assignee | Responsible | Student |
| `deliverable` | Deliverable | Deliverable | Submission |
| `phase` | Phase | Phase | Term |
| `milestone` | Milestone | Inspection Point | Exam |
| `release` | Release | Handover | Graduation |

All keys are optional. Omitted keys fall back to the default label.

---

## Changing Vocabulary

**In the UI:** Settings → Vocabulary → edit fields → Save. Changes take effect immediately
across all views for that project.

**Via API:**

```sh
curl -X PATCH http://localhost:3210/api/projects/{id} \
  -H "Content-Type: application/json" \
  -d '{"vocabulary":{"task":"Work Order","sprint":"Phase","epic":"Building"}}'
```

Only include the keys you want to change; omitted keys are left as-is.

---

## Vocabulary is Per-Project

A "Sprint" in a software project and a "Phase" in a construction project are the same underlying concept — only the label differs. Vocabulary is scoped entirely to the project and does not affect other projects.

---

## CLI Behavior

The CLI displays vocabulary-mapped labels in human-readable mode. Use `--json` to bypass labels and get raw field names:

```sh
tack list --project <id>          # shows "Work Order" instead of "Task"
tack list --project <id> --json   # returns {"item_type":"task", ...}
```

---

## Practical Tip

Set vocabulary **before** adding items. Labels appear in the item creation form, board column headers, filter dropdowns, and export files. Changing vocabulary mid-project is safe (purely cosmetic) but can cause confusion in shared contexts.

Starter vocabulary for a construction project:

```json
{
  "task":      "Work Order",
  "epic":      "Building",
  "sprint":    "Phase",
  "requirement": "Specification",
  "assignee":  "Responsible",
  "blocker":   "Hold",
  "release":   "Handover",
  "milestone": "Inspection Point"
}
```
