import { describe, expect, it } from 'vitest'

import { readData } from './yaml-values.ts'
import { at, covered } from '../mocks/spans.ts'

describe('readData()', () => {
  it('should read JSON into a value with a span for every field path', () => {
    const json = '{\n  "falls": { "caption": "Falls", "tags": ["a", 2] }\n}\n'
    const read = readData('_data/photos.json', json)

    expect(read).toEqual({
      loaded: {
        path: '_data/photos.json',
        value: { falls: { caption: 'Falls', tags: ['a', 2] } },
        spans: {
          '': at(json, '{\n  "falls": { "caption": "Falls", "tags": ["a", 2] }\n}'),
          falls: at(json, '"falls": { "caption": "Falls", "tags": ["a", 2] }'),
          'falls.caption': at(json, '"caption": "Falls"'),
          'falls.tags': at(json, '"tags": ["a", 2]'),
          'falls.tags.0': at(json, '"a"'),
          'falls.tags.1': at(json, '2'),
        },
      },
      diagnostics: [],
    })
  })

  it('should read YAML, of which JSON is a part, in its key order', () => {
    const yaml = '# photos\nb: 1\na:\n  - x\n'
    const { loaded } = readData('photos.yaml', yaml)

    expect(loaded?.value).toEqual({ b: 1, a: ['x'] })
    expect(Object.keys(loaded?.value ?? {})).toEqual(['b', 'a'])
    expect(covered(yaml, loaded!.spans[''])).toBe('b: 1\na:\n  - x')
  })

  it('should read a root that is no map, a list of lists among them', () => {
    const lists = '[[1, 2], [3]]\n'
    const scalar = '"just words"\n'

    expect(readData('lists.json', lists).loaded).toEqual({
      path: 'lists.json',
      value: [[1, 2], [3]],
      spans: {
        '': at(lists, '[[1, 2], [3]]'),
        '0': at(lists, '[1, 2]'),
        '0.0': at(lists, '1'),
        '0.1': at(lists, '2'),
        '1': at(lists, '[3]'),
        '1.0': at(lists, '3'),
      },
    })
    expect(readData('scalar.json', scalar).loaded).toEqual({
      path: 'scalar.json',
      value: 'just words',
      spans: { '': at(scalar, '"just words"') },
    })
  })

  it('should read a file of nothing but comments as null, spanning the file', () => {
    const yaml = '# nothing yet\n\n'
    const { loaded } = readData('empty.yaml', yaml)

    expect(loaded).toEqual({
      path: 'empty.yaml',
      value: null,
      spans: { '': at(yaml, '# nothing yet') },
    })
  })

  it('should report YAML that does not parse, and leave no value', () => {
    const json = '{"a": 1,,}'
    const read = readData('a.json', json)

    expect(read.loaded).toBeUndefined()
    expect(read.diagnostics).toEqual([
      expect.objectContaining({
        path: 'a.json',
        severity: 'error',
        message: expect.stringMatching(/^this data file is not valid YAML: /) as string,
      }),
    ])
  })

  it('should report what JSON cannot carry in this data file’s own words', () => {
    const read = readData('a.yaml', 'a: &x 1\nb: *x\n? [k]\n: v\nc: .inf\n')

    expect(read.loaded).toBeUndefined()
    expect(read.diagnostics.map((d) => d.message)).toEqual([
      'this data file cannot use an alias; write the value out',
      'a key in this data file must be a string',
      'this data file cannot carry Infinity: not a finite number',
    ])
  })
})
