<lem-type Note>
  text: string
</lem-type>
<lem-enum Tone = calm | urgent />

A declaration in a document takes its line, like a definition.

<lem-fn note(n: Note, tone: Tone)>
  <aside class="<lem.tone>"><lem.n.text></aside>
</lem-fn>
<lem-let n: any = {text: "Mind the gap."} />

<lem.note(n, .urgent)>
