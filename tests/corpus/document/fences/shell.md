# Keeping a branch private

Git still pushes the branch unless the config says otherwise:

```bash
# keep the notes branch off the remote
git config branch.notes.pushRemote no_push
if git diff --quiet; then echo "clean"; fi
```

Nothing else changes.
