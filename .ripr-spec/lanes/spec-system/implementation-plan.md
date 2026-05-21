# Repo-native spec rails implementation plan

Status: active
Owner: repo-architecture
Linked lane: spec-system

## End state

ripr has a repo-owned, tool-neutral durable source-of-truth namespace at `.ripr-spec/`.

## Work items

### Work item: namespace-doctrine

Status: done

#### Goal

Define ownership boundaries and create initial durable namespace scaffolding.

#### Proof commands

```bash
git diff --check
```

### Work item: templates-and-indexing

Status: ready

#### Goal

Add artifact templates and validate index/tracker consistency.

#### Proof commands

```bash
git diff --check
```
