---
topic: schemas
---

A schema may compute a field from the document it is loaded with: `readingTime: duration
= body.readingTime()` runs once, at load, and every template that shows a chip reads the
field rather than counting the words again. The count is of the text between tags, at two
hundred words a minute, rounded up — so a note this short is seconds rather than minutes,
and a site words that however it likes.

<lem-if page.readingTime.minutes() == 0>
  <p>under a minute — <lem.page.readingTime.seconds()> seconds</p>
</lem-if>
