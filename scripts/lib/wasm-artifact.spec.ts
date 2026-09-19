/**
 * The decision `npm run wasm:check` makes, fed the names a real module carries: the
 * shim for `js_sys::Date::now()` that `ALLOWED_IMPORT_NAMES` names as the violation to
 * catch, the three glue-internal imports the staged artifact actually has, and an
 * export surface with a name knocked out.
 *
 * No test here restates `ALLOWED_IMPORT_NAMES` or `EXPECTED_EXPORTS`: comparing a list
 * against a copy of itself passes forever and proves nothing, and coverage cannot tell
 * the difference, because an executed line counts either way. The lists appear only as
 * inputs.
 */

import { describe, expect, it } from 'vitest'

import { EXPECTED_EXPORTS, missingExports, unexpectedImports } from './wasm-artifact.ts'

describe('unexpectedImports()', () => {
  describe('a host capability reaching the core', () => {
    it('should report a clock shim under its hash-stripped name', () => {
      expect(unexpectedImports(['__wbg_now_0123456789abcdef'])).toEqual(['__wbg_now'])
    })

    it('should report a name that carries no hash at all', () => {
      expect(unexpectedImports(['__wbg_random'])).toEqual(['__wbg_random'])
    })

    it('should report one name for two shims of the same function', () => {
      expect(
        unexpectedImports(['__wbg_now_0123456789abcdef', '__wbg_now_fedcba9876543210'])
      ).toEqual(['__wbg_now'])
    })

    it('should report names in the order they were first seen', () => {
      expect(
        unexpectedImports(['__wbg_fetch_0123456789abcdef', '__wbg_now_fedcba9876543210'])
      ).toEqual(['__wbg_fetch', '__wbg_now'])
    })
  })

  describe('the imports the staged artifact carries today', () => {
    it('should pass a hashed shim whose stripped name is allowlisted', () => {
      expect(unexpectedImports(['__wbg_Error_408e67f47ca7b58b'])).toEqual([])
    })

    it('should pass a shim whose own name holds a second wbindgen prefix', () => {
      expect(unexpectedImports(['__wbg___wbindgen_throw_bb96b2010945f0bc'])).toEqual([])
    })

    it('should pass an intrinsic that wasm-bindgen leaves unhashed', () => {
      expect(unexpectedImports(['__wbindgen_init_externref_table'])).toEqual([])
    })

    it('should pass the whole set together', () => {
      expect(
        unexpectedImports([
          '__wbg___wbindgen_throw_bb96b2010945f0bc',
          '__wbg_Error_408e67f47ca7b58b',
          '__wbindgen_init_externref_table',
        ])
      ).toEqual([])
    })

    it('should report nothing for a module that imports nothing', () => {
      expect(unexpectedImports([])).toEqual([])
    })
  })

  // Fails closed on purpose: were wasm-bindgen's suffix to move, no shim name would
  // match the allowlist any more and every import would be reported — louder than a
  // loose match, which would let a real capability through under a stripped name.
  describe('what the hash stripping accepts', () => {
    it('should leave a fifteen-digit suffix in place', () => {
      expect(unexpectedImports(['__wbg_Error_408e67f47ca7b58'])).toEqual([
        '__wbg_Error_408e67f47ca7b58',
      ])
    })

    it('should leave an uppercase suffix in place', () => {
      expect(unexpectedImports(['__wbg_Error_408E67F47CA7B58B'])).toEqual([
        '__wbg_Error_408E67F47CA7B58B',
      ])
    })

    it('should strip a suffix only at the end of the name', () => {
      expect(unexpectedImports(['__wbg_408e67f47ca7b58b_Error'])).toEqual([
        '__wbg_408e67f47ca7b58b_Error',
      ])
    })

    it('should strip one suffix, not every hash-shaped run', () => {
      expect(unexpectedImports(['__wbg_Error_0123456789abcdef_408e67f47ca7b58b'])).toEqual([
        '__wbg_Error_0123456789abcdef',
      ])
    })
  })
})

describe('missingExports()', () => {
  it('should report an expected export the module does not carry', () => {
    const surface = EXPECTED_EXPORTS.filter((name) => name !== 'core_render')
    expect(missingExports(surface)).toEqual(['core_render'])
  })

  it('should report nothing when every expected export is present', () => {
    expect(missingExports(EXPECTED_EXPORTS)).toEqual([])
  })

  it('should ignore the runtime internals a module also exports', () => {
    expect(missingExports(['memory', ...EXPECTED_EXPORTS, '__wbindgen_malloc'])).toEqual([])
  })

  it('should report every missing name, in the order the expected list has them', () => {
    const surface = EXPECTED_EXPORTS.filter(
      (name) => name !== 'version' && name !== 'core_new'
    )
    expect(missingExports(surface)).toEqual(['version', 'core_new'])
  })

  it('should compare export names verbatim, without stripping a hash', () => {
    const surface = EXPECTED_EXPORTS.map((name) =>
      name === 'core_render' ? 'core_render_0123456789abcdef' : name
    )
    expect(missingExports(surface)).toEqual(['core_render'])
  })
})
