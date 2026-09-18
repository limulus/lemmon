---
topic: git
---

A branch a public repo should never see can live in a private fork, added as a second
remote with the branch's push remote pointed at it:

```bash
git remote add drafts git@github.com:limulus/drafts.git
git config branch.notes.pushRemote drafts
```

<lem.page.readingTime.seconds()>
