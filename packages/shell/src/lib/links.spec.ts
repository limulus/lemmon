import { describe, expect, it } from 'vitest'

import { refusal, srcset } from './links.ts'

describe('srcset()', () => {
  it("should name the descriptor parse-srcset refused where the srcset holds ` at '` itself", () => {
    expect(srcset("/a.jpg 1x, /b.jpg at '2x'")).toEqual({
      candidates: [{ url: '/a.jpg', d: 1 }],
      refused: ["'2x'"],
    })
  })
})

describe('refusal()', () => {
  const value = 'a.jpg 2q'

  it('should read the descriptor from the message parse-srcset 1.0.2 logs', () => {
    expect(refusal(value, "Invalid srcset descriptor found in 'a.jpg 2q' at '2q'.")).toBe(
      '2q'
    )
  })

  it('should keep a message of any other shape whole rather than throw', () => {
    for (const message of [
      'srcset: descriptor 2q is invalid',
      "Invalid srcset descriptor found in 'b.jpg 2q' at '2q'.",
      "Invalid srcset descriptor found in 'a.jpg 2q' at '2q'",
    ]) {
      expect(refusal(value, message)).toBe(message)
    }
  })
})
