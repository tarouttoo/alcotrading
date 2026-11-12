Your whole reply is one `<output>` block, written straight from the exploration you have already done:

```json
<output>
{
  "issues": [
    {
      "number": 101,
      "tag": "easy-call | needs-a-human | blocked",
      "duplicate_of": null,
      "comment": "The survivor's full markdown comment, per step 5. COMMENT."
    },
    {
      "number": 117,
      "tag": "easy-call",
      "duplicate_of": 101,
      "comment": "Same missing guard in Z; the CLI path just reaches it first."
    }
  ],
  "patterns": "Optional markdown: cross-group patterns worth a maintainer's attention."
}
</output>
```

The workflow rejects the whole plan unless:

- Every issue in the batch appears exactly once. `number` and `duplicate_of` are integers (or `duplicate_of` is `null`).
- `duplicate_of` names another batch issue that is itself a survivor.
- `tag` is one of the three above; `comment` is non-empty.
