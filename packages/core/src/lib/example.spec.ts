import { describe, it, expect, vi } from 'vitest'

import { Example } from './example.ts'

describe('Example', () => {
  describe('hello()', () => {
    it('should write a greeting to stdout', () => {
      const log = vi.spyOn(console, 'log').mockImplementation(() => {})

      new Example().hello()

      expect(log).toHaveBeenCalledWith('hello world')
    })
  })
})
